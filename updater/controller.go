package main

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"strings"
	"sync/atomic"
	"time"

	"github.com/keel-hq/keel/provider"
	"github.com/keel-hq/keel/registry"
	"github.com/keel-hq/keel/trigger/poll"
	"github.com/keel-hq/keel/types"
	"github.com/opencontainers/go-digest"
	corev1 "k8s.io/api/core/v1"
	apierrors "k8s.io/apimachinery/pkg/api/errors"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/apis/meta/v1/unstructured"
	"k8s.io/apimachinery/pkg/runtime"
	k8stypes "k8s.io/apimachinery/pkg/types"
	"k8s.io/client-go/dynamic"
	coreclient "k8s.io/client-go/kubernetes/typed/core/v1"
	"k8s.io/client-go/util/retry"
	"k8s.io/component-helpers/scheduling/corev1/nodeaffinity"
)

type scheduled struct {
	key  string
	next time.Time
}

type controller struct {
	api       dynamic.Interface
	core      coreclient.CoreV1Interface
	namespace string
	plain     map[string]bool
	jobs      map[k8stypes.UID]scheduled
	ready     atomic.Bool
	registry  func(context.Context, *target) (registry.Client, error)
}

func (c *controller) scan(ctx context.Context, now time.Time) error {
	objects, err := c.api.Resource(clusters).Namespace(c.namespace).List(ctx, metav1.ListOptions{})
	if err != nil {
		c.ready.Store(false)
		if apierrors.IsNotFound(err) {
			clear(c.jobs)
			return nil // Installing the operator later requires no updater restart.
		}
		return err
	}
	c.ready.Store(true)
	seen := map[k8stypes.UID]bool{}
	for i := range objects.Items {
		object := &objects.Items[i]
		selected, err := selected(object)
		if err != nil {
			slog.Warn("database update settings refused", "namespace", object.GetNamespace(), "name", object.GetName(), "error", err)
		}
		if selected == nil {
			continue
		}
		uid := object.GetUID()
		seen[uid] = true
		job, exists := c.jobs[uid]
		if !exists || job.key != selected.key {
			c.jobs[uid] = scheduled{key: selected.key, next: selected.schedule.Next(now.UTC())}
			continue
		}
		if job.next.IsZero() || now.Before(job.next) {
			continue
		}
		c.jobs[uid] = scheduled{key: selected.key, next: selected.schedule.Next(now.UTC())}
		checkCtx, cancel := context.WithTimeout(ctx, 45*time.Second)
		err = c.check(checkCtx, selected, now)
		cancel()
		if err != nil {
			slog.Error("database update check failed", "namespace", object.GetNamespace(), "name", object.GetName(), "error", err)
		}
	}
	for uid := range c.jobs {
		if !seen[uid] {
			delete(c.jobs, uid)
		}
	}
	return nil
}

func (c *controller) check(ctx context.Context, selected *target, now time.Time) error {
	if !healthy(selected.object) {
		return fmt.Errorf("database is not ready; waiting for its next scheduled check")
	}
	platforms, running, err := c.runtime(ctx, selected)
	if err != nil {
		return err
	}
	client, err := c.registry(ctx, selected)
	if err != nil {
		return err
	}
	tracked := &types.TrackedImage{
		Image: selected.ref, Policy: sameTag{}, Trigger: types.TriggerTypePoll,
		PollSchedule: selected.object.GetAnnotations()[types.KeelPollScheduleAnnotation],
		Namespace:    selected.object.GetNamespace(), Provider: "cloudnative-pg",
		Platforms: platforms, RunningDigests: running,
	}
	sink := &scopedProvider{ctx: ctx, controller: c, target: selected, tracked: tracked, registry: client, now: now}
	// Upstream Watch performs one immediate check before registering its cron job. We never start
	// that cron: this provider belongs to one due Cluster, and is discarded after this check. Its
	// baseline is rebuilt from persisted state, so a failed write is retried on the next schedule.
	watcher := poll.NewRepositoryWatcher(sink, client)
	if err := watcher.Watch(tracked); err != nil {
		return err
	}
	return sink.err
}

type scopedProvider struct {
	ctx        context.Context
	controller *controller
	target     *target
	tracked    *types.TrackedImage
	registry   registry.Client
	now        time.Time
	err        error
}

var _ provider.Providers = (*scopedProvider)(nil)

func (p *scopedProvider) List() []string { return []string{"cloudnative-pg"} }
func (p *scopedProvider) Stop()          {}
func (p *scopedProvider) TrackedImages() ([]*types.TrackedImage, error) {
	return []*types.TrackedImage{p.tracked}, nil
}

func (p *scopedProvider) Submit(event types.Event) error {
	p.err = p.apply(event)
	return p.err
}

func (p *scopedProvider) apply(event types.Event) error {
	if event.TriggerName != types.TriggerTypePoll.String() || event.Repository.Name != p.target.ref.Repository() || event.Repository.Tag != p.target.ref.Tag() {
		return fmt.Errorf("refusing an event outside this database's scheduled check")
	}
	if _, err := digest.Parse(event.Repository.Digest); err != nil {
		return fmt.Errorf("registry returned an invalid digest: %w", err)
	}
	platforms, err := p.registry.Platforms(registry.Opts{Name: p.target.ref.ShortName(), Tag: event.Repository.Digest})
	if err != nil {
		return err
	}
	if !types.PlatformsSupportAll(platforms, p.tracked.Platforms) {
		return fmt.Errorf("candidate image does not support every eligible database node")
	}
	tagged, _, _ := strings.Cut(p.target.image, "@")
	desired := tagged + "@" + event.Repository.Digest
	api := p.controller.api.Resource(clusters).Namespace(p.target.object.GetNamespace())
	err = retry.RetryOnConflict(retry.DefaultRetry, func() error {
		current, err := api.Get(p.ctx, p.target.object.GetName(), metav1.GetOptions{})
		if err != nil {
			return err
		}
		latest, err := selected(current)
		if err != nil || latest == nil || latest.key != p.target.key || !healthy(current) {
			return fmt.Errorf("database identity, image, ownership, readiness or update settings changed")
		}
		if latest.image == desired {
			return nil
		}
		patch, err := json.Marshal(map[string]interface{}{
			"metadata": map[string]interface{}{
				"resourceVersion": current.GetResourceVersion(),
				"uid":             string(current.GetUID()),
				"annotations": map[string]string{
					types.KeelUpdateTimeAnnotation: p.now.UTC().Format(time.RFC3339),
					types.KeelDigestAnnotation:     event.Repository.Digest,
					"kubernetes.io/change-cause":   "Zeabur database updater: " + desired,
				},
			},
			"spec": map[string]string{"imageName": desired},
		})
		if err != nil {
			return err
		}
		_, err = api.Patch(p.ctx, current.GetName(), k8stypes.MergePatchType, patch, metav1.PatchOptions{FieldManager: "zeabur-database-updater"})
		return err
	})
	if err == nil {
		slog.Info("database image updated", "namespace", p.target.object.GetNamespace(), "name", p.target.object.GetName(), "image", desired)
	}
	return err
}

func unstructuredSpec(selected *target) (map[string]interface{}, bool, error) {
	return unstructured.NestedMap(selected.object.Object, "spec")
}

func healthy(object *unstructured.Unstructured) bool {
	conditions, _, _ := unstructured.NestedSlice(object.Object, "status", "conditions")
	for _, condition := range conditions {
		value, ok := condition.(map[string]interface{})
		if ok && value["type"] == "Ready" && value["status"] == "True" {
			instances, _, _ := unstructured.NestedInt64(object.Object, "spec", "instances")
			ready, _, _ := unstructured.NestedInt64(object.Object, "status", "readyInstances")
			return instances > 0 && ready == instances
		}
	}
	return false
}

func (c *controller) runtime(ctx context.Context, selected *target) ([]types.Platform, []string, error) {
	var spec struct {
		Affinity struct {
			NodeSelector map[string]string    `json:"nodeSelector"`
			NodeAffinity *corev1.NodeAffinity `json:"nodeAffinity"`
		} `json:"affinity"`
	}
	value, _, err := unstructuredSpec(selected)
	if err != nil {
		return nil, nil, err
	}
	if err := runtime.DefaultUnstructuredConverter.FromUnstructured(value, &spec); err != nil {
		return nil, nil, err
	}
	nodes, err := c.core.Nodes().List(ctx, metav1.ListOptions{})
	if err != nil {
		return nil, nil, err
	}
	pod := &corev1.Pod{Spec: corev1.PodSpec{NodeSelector: spec.Affinity.NodeSelector, Affinity: &corev1.Affinity{NodeAffinity: spec.Affinity.NodeAffinity}}}
	constraint := nodeaffinity.GetRequiredNodeAffinity(pod)
	platforms := []types.Platform{}
	for i := range nodes.Items {
		node := &nodes.Items[i]
		matches, err := constraint.Match(node)
		if err != nil {
			return nil, nil, err
		}
		if !matches || node.Status.NodeInfo.OperatingSystem != "linux" {
			continue
		}
		if node.Status.NodeInfo.Architecture == "" {
			return nil, nil, fmt.Errorf("node %s has no architecture", node.Name)
		}
		platforms = append(platforms, types.Platform{OS: "linux", Architecture: node.Status.NodeInfo.Architecture})
	}
	if len(platforms) == 0 {
		return nil, nil, fmt.Errorf("no eligible Linux nodes for the database")
	}
	if selected.pin != "" {
		return platforms, []string{selected.pin}, nil
	}
	pods, err := c.core.Pods(selected.object.GetNamespace()).List(ctx, metav1.ListOptions{LabelSelector: "cnpg.io/cluster=" + selected.object.GetName()})
	if err != nil {
		return nil, nil, err
	}
	running := []string{}
	for _, pod := range pods.Items {
		owner := metav1.GetControllerOf(&pod)
		if owner == nil || owner.UID != selected.object.GetUID() {
			continue
		}
		for _, container := range pod.Status.ContainerStatuses {
			if container.Name != "postgres" || !container.Ready {
				continue
			}
			id := container.ImageID
			if _, tail, ok := strings.Cut(id, "://"); ok {
				id = tail
			}
			if _, tail, ok := strings.Cut(id, "@"); ok {
				id = tail
			}
			if _, err := digest.Parse(id); err != nil {
				return nil, nil, fmt.Errorf("postgres pod %s has no usable runtime digest", pod.Name)
			}
			running = append(running, id)
		}
	}
	if len(running) == 0 {
		return nil, nil, fmt.Errorf("database has no observed running image digest")
	}
	return platforms, running, nil
}
