package main

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"

	"github.com/keel-hq/keel/registry"
	"github.com/keel-hq/keel/registry/docker"
	"github.com/keel-hq/keel/types"
	corev1 "k8s.io/api/core/v1"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/runtime"
	coreclient "k8s.io/client-go/kubernetes/typed/core/v1"
)

// Each check gets a credential-bound client. Keel remains responsible for registry authentication,
// manifest parsing and platform metadata; the adapter supplies a deadline and explicit HTTP hosts.
type registryClient struct {
	hub *docker.Registry
}

func newRegistry(ctx context.Context, ref *target, core coreclient.CoreV1Interface, plain map[string]bool) (registry.Client, error) {
	host := ref.ref.Registry()
	username, password, err := credentials(ctx, core, ref)
	if err != nil {
		return nil, err
	}
	scheme := "https"
	if plain[host] {
		scheme = "http"
	}
	hub := docker.New(scheme+"://"+host, username, password)
	hub.Client.Timeout = 15 * time.Second
	hub.Client.Transport = &boundedTransport{ctx: ctx, next: hub.Client.Transport}
	hub.Logf = func(string, ...interface{}) {}
	return &registryClient{hub: hub}, nil
}

type boundedTransport struct {
	ctx  context.Context
	next http.RoundTripper
}

func (t *boundedTransport) RoundTrip(request *http.Request) (*http.Response, error) {
	// Retain net/http's per-request timeout while also stopping the whole registry check.
	ctx, cancel := context.WithCancel(request.Context())
	stop := context.AfterFunc(t.ctx, cancel)
	finish := func() { stop(); cancel() }
	response, err := t.next.RoundTrip(request.Clone(ctx))
	if response != nil && response.Body != nil {
		response.Body = &boundedBody{ReadCloser: http.MaxBytesReader(nil, response.Body, 8<<20), finish: finish}
	} else {
		finish()
	}
	return response, err
}

type boundedBody struct {
	io.ReadCloser
	finish func()
}

func (b *boundedBody) Close() error {
	defer b.finish()
	return b.ReadCloser.Close()
}

func (r *registryClient) Digest(opts registry.Opts) (string, error) {
	all, err := r.Digests(opts)
	if err != nil {
		return "", err
	}
	if len(all) == 0 {
		return "", fmt.Errorf("registry returned no digest")
	}
	return all[0], nil
}

func (r *registryClient) Digests(opts registry.Opts) ([]string, error) {
	return r.hub.ManifestDigests(opts.Name, opts.Tag)
}

func (r *registryClient) Platforms(opts registry.Opts) ([]types.Platform, error) {
	return r.hub.ManifestPlatforms(opts.Name, opts.Tag)
}

func (r *registryClient) Get(registry.Opts) (*registry.Repository, error) {
	return nil, fmt.Errorf("database updates never enumerate or change image tags")
}

func credentials(ctx context.Context, core coreclient.CoreV1Interface, selected *target) (string, string, error) {
	var spec struct {
		ImagePullSecrets []corev1.LocalObjectReference `json:"imagePullSecrets"`
	}
	value, _, _ := unstructuredSpec(selected)
	if err := runtime.DefaultUnstructuredConverter.FromUnstructured(value, &spec); err != nil {
		return "", "", err
	}
	name := selected.object.GetLabels()[types.KeelImagePullSecretAnnotation]
	if name == "" {
		name = selected.object.GetAnnotations()[types.KeelImagePullSecretAnnotation]
	}
	if name != "" {
		spec.ImagePullSecrets = append(spec.ImagePullSecrets, corev1.LocalObjectReference{Name: name})
	}
	for _, reference := range spec.ImagePullSecrets {
		secret, err := core.Secrets(selected.object.GetNamespace()).Get(ctx, reference.Name, metav1.GetOptions{})
		if err != nil {
			return "", "", fmt.Errorf("cannot read image pull secret %s: %w", reference.Name, err)
		}
		var config struct {
			Auths map[string]struct {
				Username string `json:"username"`
				Password string `json:"password"`
				Auth     string `json:"auth"`
			} `json:"auths"`
		}
		if err := json.Unmarshal(secret.Data[corev1.DockerConfigJsonKey], &config); err != nil {
			return "", "", fmt.Errorf("invalid image pull secret %s", reference.Name)
		}
		for address, auth := range config.Auths {
			address = strings.TrimPrefix(strings.TrimPrefix(address, "https://"), "http://")
			address = strings.TrimSuffix(strings.TrimSuffix(address, "/v1/"), "/")
			if address == "index.docker.io" || address == "docker.io" {
				address = "index.docker.io"
			}
			if address != selected.ref.Registry() {
				continue
			}
			if auth.Auth != "" {
				decoded, err := base64.StdEncoding.DecodeString(auth.Auth)
				if err != nil {
					return "", "", fmt.Errorf("invalid registry credential in %s", reference.Name)
				}
				username, password, ok := strings.Cut(string(decoded), ":")
				if !ok {
					return "", "", fmt.Errorf("invalid registry credential in %s", reference.Name)
				}
				return username, password, nil
			}
			return auth.Username, auth.Password, nil
		}
	}
	return "", "", nil
}
