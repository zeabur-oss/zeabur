package main

import (
	"context"
	"errors"
	"reflect"
	"strings"
	"testing"
	"time"

	"github.com/keel-hq/keel/registry"
	"github.com/keel-hq/keel/types"
	corev1 "k8s.io/api/core/v1"
	apierrors "k8s.io/apimachinery/pkg/api/errors"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/apis/meta/v1/unstructured"
	"k8s.io/apimachinery/pkg/runtime"
	"k8s.io/apimachinery/pkg/runtime/schema"
	k8stypes "k8s.io/apimachinery/pkg/types"
	dynamicfake "k8s.io/client-go/dynamic/fake"
	kubefake "k8s.io/client-go/kubernetes/fake"
	k8stesting "k8s.io/client-go/testing"
)

const oldDigest = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
const newDigest = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
const taggedImage = "registry.example/postgresql:17-standard-trixie"

var start = time.Date(2026, 11, 27, 0, 0, 0, 0, time.UTC)

func database(t *testing.T, name, schedule string) *unstructured.Unstructured {
	t.Helper()
	cluster := &unstructured.Unstructured{}
	if err := cluster.UnmarshalJSON([]byte(`{
		"apiVersion":"postgresql.cnpg.io/v1","kind":"Cluster",
		"metadata":{"name":"pg","namespace":"test","uid":"uid","resourceVersion":"1",
			"annotations":{"keel.sh/policy":"force","keel.sh/trigger":"poll","keel.sh/match-tag":"true",
				"keel.sh/pollSchedule":"@every 1m","kubectl.kubernetes.io/last-applied-configuration":"original","unrelated":"keep"}},
		"spec":{"imageName":"placeholder","instances":3,"storage":{"size":"1Gi"},"backup":{"keep":"all"}},
		"status":{"readyInstances":3,"conditions":[{"type":"Ready","status":"True"}]}
	}`)); err != nil {
		t.Fatal(err)
	}
	cluster.SetName(name)
	cluster.SetUID(k8stypes.UID(name))
	annotations := cluster.GetAnnotations()
	annotations[types.KeelPollScheduleAnnotation] = schedule
	cluster.SetAnnotations(annotations)
	_ = unstructured.SetNestedField(cluster.Object, taggedImage+"@"+oldDigest, "spec", "imageName")
	return cluster
}

type fakeRegistry struct {
	digest    string
	platforms []types.Platform
	err       error
}

func (r *fakeRegistry) Digest(registry.Opts) (string, error)              { return r.digest, r.err }
func (r *fakeRegistry) Digests(registry.Opts) ([]string, error)           { return []string{r.digest}, r.err }
func (r *fakeRegistry) Platforms(registry.Opts) ([]types.Platform, error) { return r.platforms, r.err }
func (*fakeRegistry) Get(registry.Opts) (*registry.Repository, error) {
	return nil, errors.New("must not enumerate tags")
}

func setup(t *testing.T, objects ...runtime.Object) (*controller, *dynamicfake.FakeDynamicClient, *fakeRegistry, *int) {
	t.Helper()
	api := dynamicfake.NewSimpleDynamicClientWithCustomListKinds(runtime.NewScheme(), map[schema.GroupVersionResource]string{clusters: "ClusterList"}, objects...)
	core := kubefake.NewSimpleClientset(&corev1.Node{ObjectMeta: metav1.ObjectMeta{Name: "node"}, Status: corev1.NodeStatus{NodeInfo: corev1.NodeSystemInfo{OperatingSystem: "linux", Architecture: "amd64"}}})
	registryClient := &fakeRegistry{digest: newDigest, platforms: []types.Platform{{OS: "linux", Architecture: "amd64"}}}
	checks := new(int)
	c := &controller{api: api, core: core.CoreV1(), namespace: "test", jobs: map[k8stypes.UID]scheduled{}}
	c.registry = func(context.Context, *target) (registry.Client, error) { *checks++; return registryClient, nil }
	return c, api, registryClient, checks
}

func imageName(t *testing.T, api *dynamicfake.FakeDynamicClient, name string) string {
	t.Helper()
	object, err := api.Resource(clusters).Namespace("test").Get(context.Background(), name, metav1.GetOptions{})
	if err != nil {
		t.Fatal(err)
	}
	image, _, err := unstructured.NestedString(object.Object, "spec", "imageName")
	if err != nil {
		t.Fatal(err)
	}
	return image
}

func TestSchedulesAreIndependentAndSurviveRestartWithoutImmediateUpdates(t *testing.T) {
	active := database(t, "active", "@every 1m")
	off := database(t, "off", "@every 1m")
	off.SetAnnotations(map[string]string{})
	future := database(t, "future", "0 0 3 * * *")
	never := database(t, "never", "0 0 0 31 2 *")
	c, api, _, checks := setup(t, active, off, future, never)
	if err := c.scan(context.Background(), start); err != nil {
		t.Fatal(err)
	}
	if *checks != 0 {
		t.Fatal("polled at startup")
	}
	if err := c.scan(context.Background(), start.Add(time.Minute)); err != nil {
		t.Fatal(err)
	}
	if *checks != 1 {
		t.Fatalf("wanted one due database, got %d", *checks)
	}
	if got := imageName(t, api, "active"); got != taggedImage+"@"+newDigest {
		t.Fatal(got)
	}
	for _, name := range []string{"off", "future", "never"} {
		if got := imageName(t, api, name); got != taggedImage+"@"+oldDigest {
			t.Fatalf("%s changed: %s", name, got)
		}
	}
	c.jobs = map[k8stypes.UID]scheduled{}
	if err := c.scan(context.Background(), start.Add(2*time.Minute)); err != nil {
		t.Fatal(err)
	}
	if *checks != 1 {
		t.Fatal("polled immediately after restart")
	}
}

func TestUpdatePreservesSpecAndMetadataAcrossStatusConflict(t *testing.T) {
	before := database(t, "active", "@every 1m")
	c, api, _, _ := setup(t, before)
	conflicts := 0
	api.PrependReactor("patch", "clusters", func(action k8stesting.Action) (bool, runtime.Object, error) {
		if conflicts > 0 {
			return false, nil, nil
		}
		conflicts++
		changed := before.DeepCopy()
		changed.SetResourceVersion("2")
		_ = unstructured.SetNestedField(changed.Object, "status changed", "status", "phaseReason")
		annotations := changed.GetAnnotations()
		annotations["after-check"] = "keep"
		changed.SetAnnotations(annotations)
		if err := api.Tracker().Update(clusters, changed, "test"); err != nil {
			t.Fatal(err)
		}
		return true, nil, apierrors.NewConflict(clusters.GroupResource(), "active", errors.New("status churn"))
	})
	_ = c.scan(context.Background(), start)
	_ = c.scan(context.Background(), start.Add(time.Minute))
	after, err := api.Resource(clusters).Namespace("test").Get(context.Background(), "active", metav1.GetOptions{})
	if err != nil {
		t.Fatal(err)
	}
	if imageName(t, api, "active") != taggedImage+"@"+newDigest || conflicts != 1 {
		t.Fatal("update did not retry")
	}
	wantedSpec, _, _ := unstructured.NestedMap(before.Object, "spec")
	wantedSpec["imageName"] = taggedImage + "@" + newDigest
	gotSpec, _, _ := unstructured.NestedMap(after.Object, "spec")
	if !reflect.DeepEqual(gotSpec, wantedSpec) {
		t.Fatalf("spec changed: %#v", gotSpec)
	}
	for key, value := range before.GetAnnotations() {
		if after.GetAnnotations()[key] != value {
			t.Fatalf("annotation %s lost", key)
		}
	}
	if after.GetAnnotations()["after-check"] != "keep" {
		t.Fatal("concurrent annotation lost")
	}
}

func TestFailedWriteIsRetriedAtNextSchedule(t *testing.T) {
	c, api, _, _ := setup(t, database(t, "active", "@every 1m"))
	fail := true
	api.PrependReactor("patch", "clusters", func(k8stesting.Action) (bool, runtime.Object, error) {
		if fail {
			return true, nil, apierrors.NewForbidden(clusters.GroupResource(), "active", errors.New("temporary refusal"))
		}
		return false, nil, nil
	})
	_ = c.scan(context.Background(), start)
	_ = c.scan(context.Background(), start.Add(time.Minute))
	if imageName(t, api, "active") != taggedImage+"@"+oldDigest {
		t.Fatal("failed patch applied")
	}
	fail = false
	_ = c.scan(context.Background(), start.Add(2*time.Minute))
	if imageName(t, api, "active") != taggedImage+"@"+newDigest {
		t.Fatal("failed write consumed the update")
	}
}

func TestDisableOrChangedTargetDuringRegistryReadRefusesWrite(t *testing.T) {
	changes := map[string]func(*unstructured.Unstructured){
		"disable": func(o *unstructured.Unstructured) { o.SetAnnotations(map[string]string{}) },
		"schedule": func(o *unstructured.Unstructured) {
			a := o.GetAnnotations()
			a[types.KeelPollScheduleAnnotation] = "@daily"
			o.SetAnnotations(a)
		},
		"recreated": func(o *unstructured.Unstructured) { o.SetUID("different") },
		"catalog": func(o *unstructured.Unstructured) {
			_ = unstructured.SetNestedMap(o.Object, map[string]interface{}{"name": "catalog", "major": int64(17)}, "spec", "imageCatalogRef")
		},
		"managed": func(o *unstructured.Unstructured) {
			o.SetLabels(map[string]string{"app.kubernetes.io/managed-by": "Helm"})
		},
		"unready": func(o *unstructured.Unstructured) {
			_ = unstructured.SetNestedField(o.Object, int64(2), "status", "readyInstances")
		},
		"approvals": func(o *unstructured.Unstructured) {
			o.SetLabels(map[string]string{"keel.sh/approvals": "1"})
		},
	}
	for name, mutate := range changes {
		t.Run(name, func(t *testing.T) {
			before := database(t, "active", "@every 1m")
			c, api, client, _ := setup(t, before)
			c.registry = func(context.Context, *target) (registry.Client, error) {
				changed := before.DeepCopy()
				mutate(changed)
				err := api.Tracker().Update(clusters, changed, "test")
				return client, err
			}
			_ = c.scan(context.Background(), start)
			_ = c.scan(context.Background(), start.Add(time.Minute))
			if imageName(t, api, "active") != taggedImage+"@"+oldDigest {
				t.Fatal("stale target was changed")
			}
		})
	}
}

func TestUnsupportedPlatformAndInvalidDigestNeverWrite(t *testing.T) {
	for _, mode := range []string{"platform", "digest", "registry"} {
		t.Run(mode, func(t *testing.T) {
			c, api, client, _ := setup(t, database(t, "active", "@every 1m"))
			switch mode {
			case "platform":
				client.platforms = []types.Platform{{OS: "linux", Architecture: "arm64"}}
			case "digest":
				client.digest = "invalid"
			case "registry":
				client.err = errors.New("registry unavailable")
			}
			_ = c.scan(context.Background(), start)
			_ = c.scan(context.Background(), start.Add(time.Minute))
			if !strings.HasSuffix(imageName(t, api, "active"), oldDigest) {
				t.Fatal("unsafe image applied")
			}
		})
	}
}
