package main

import (
	"encoding/json"
	"fmt"
	"regexp"
	"strings"

	"github.com/keel-hq/keel/types"
	"github.com/keel-hq/keel/util/image"
	"github.com/opencontainers/go-digest"
	"github.com/rusenask/cron"
	"k8s.io/apimachinery/pkg/apis/meta/v1/unstructured"
	"k8s.io/apimachinery/pkg/runtime/schema"
)

var clusters = schema.GroupVersionResource{Group: "postgresql.cnpg.io", Version: "v1", Resource: "clusters"}
var majorTag = regexp.MustCompile(`^v?([1-9][0-9]+)([.-]|$)`)

var controlKeys = []string{
	types.KeelPolicyLabel, types.KeelTriggerLabel, types.KeelPollScheduleAnnotation,
	types.KeelForceTagMatchLegacyLabel, types.KeelForceTagMatchLabel,
}

var settingKeys = append(controlKeys, types.KeelMinimumApprovalsLabel, types.KeelImagePullSecretAnnotation)

type target struct {
	object   *unstructured.Unstructured
	image    string
	pin      string
	ref      *image.Reference
	schedule cron.Schedule
	key      string
}

func selected(object *unstructured.Unstructured) (*target, error) {
	a, labels := object.GetAnnotations(), object.GetLabels()
	if object.GetUID() == "" || object.GetDeletionTimestamp() != nil || managed(object) {
		return nil, nil
	}
	for _, key := range controlKeys {
		if labels[key] != "" {
			return nil, fmt.Errorf("legacy Keel labels must be removed before enabling database updates")
		}
	}
	if a[types.KeelPolicyLabel] != "force" || a[types.KeelTriggerLabel] != "poll" {
		return nil, nil
	}
	match := a[types.KeelForceTagMatchLabel]
	if match == "" {
		match = a[types.KeelForceTagMatchLegacyLabel]
	}
	if match != "true" {
		return nil, fmt.Errorf("database updates require same-tag polling without approvals")
	}
	for _, values := range []map[string]string{a, labels} {
		if approvals := values[types.KeelMinimumApprovalsLabel]; approvals != "" && approvals != "0" {
			return nil, fmt.Errorf("database updates do not support approval requirements")
		}
	}
	catalog, _, err := unstructured.NestedFieldNoCopy(object.Object, "spec", "imageCatalogRef")
	if err != nil || catalog != nil {
		return nil, nil
	}
	name, _, err := unstructured.NestedString(object.Object, "spec", "imageName")
	if err != nil {
		return nil, err
	}
	tagged, pin, pinned := strings.Cut(name, "@")
	colon := strings.LastIndex(tagged, ":")
	if colon <= strings.LastIndex(tagged, "/") || !majorTag.MatchString(tagged[colon+1:]) {
		return nil, fmt.Errorf("database image needs an explicit PostgreSQL major-version tag")
	}
	if pinned {
		if _, err := digest.Parse(pin); err != nil {
			return nil, fmt.Errorf("invalid database image digest: %w", err)
		}
	}
	ref, err := image.Parse(tagged)
	if err != nil {
		return nil, err
	}
	schedule, err := cron.Parse(a[types.KeelPollScheduleAnnotation])
	if err != nil {
		return nil, fmt.Errorf("invalid database update schedule: %w", err)
	}
	settings := map[string]string{"uid": string(object.GetUID()), "image": name}
	for _, key := range settingKeys {
		settings[key] = a[key]
		settings["label:"+key] = labels[key]
	}
	// Credentials and placement are part of the reviewed target, too.
	for _, key := range []string{"imagePullSecrets", "affinity"} {
		value, _, _ := unstructured.NestedFieldNoCopy(object.Object, "spec", key)
		encoded, _ := json.Marshal(value)
		settings[key] = string(encoded)
	}
	key, err := json.Marshal(settings)
	if err != nil {
		return nil, err
	}
	return &target{object: object, image: name, pin: pin, ref: ref, schedule: schedule, key: string(key)}, nil
}

func managed(object *unstructured.Unstructured) bool {
	if len(object.GetOwnerReferences()) != 0 {
		return true
	}
	labels, annotations := object.GetLabels(), object.GetAnnotations()
	for _, key := range []string{"app.kubernetes.io/managed-by", "helm.sh/chart", "argocd.argoproj.io/instance", "kustomize.toolkit.fluxcd.io/name", "helm.toolkit.fluxcd.io/name"} {
		if labels[key] != "" {
			return true
		}
	}
	for _, key := range []string{"meta.helm.sh/release-name", "argocd.argoproj.io/tracking-id"} {
		if annotations[key] != "" {
			return true
		}
	}
	return false
}

// The adapter accepts only force+match-tag; other Keel policies cannot select a database version.
type sameTag struct{}

func (sameTag) KeepTag() bool                          { return true }
func (sameTag) Name() string                           { return "force" }
func (sameTag) Type() types.PolicyType                 { return types.PolicyTypeForce }
func (sameTag) Filter(tags []string) []string          { return tags }
func (sameTag) ShouldUpdate(a, b string) (bool, error) { return a == b, nil }
