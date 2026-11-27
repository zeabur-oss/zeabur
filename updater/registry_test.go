package main

import (
	"context"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/keel-hq/keel/registry"
	"github.com/keel-hq/keel/types"
	corev1 "k8s.io/api/core/v1"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/apis/meta/v1/unstructured"
	kubefake "k8s.io/client-go/kubernetes/fake"
)

func TestRegistryRequestsRespectBothDeadlines(t *testing.T) {
	for _, canceled := range []string{"check", "request"} {
		t.Run(canceled, func(t *testing.T) {
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				<-r.Context().Done()
			}))
			defer server.Close()
			check, cancel := context.WithCancel(context.Background())
			defer cancel()
			request, stop := context.WithCancel(context.Background())
			defer stop()
			client := &http.Client{Transport: &boundedTransport{ctx: check, next: http.DefaultTransport}}
			req, _ := http.NewRequestWithContext(request, http.MethodGet, server.URL, nil)
			done := make(chan error, 1)
			go func() {
				response, err := client.Do(req)
				if response != nil {
					response.Body.Close()
				}
				done <- err
			}()
			if canceled == "check" {
				cancel()
			} else {
				stop()
			}
			select {
			case err := <-done:
				if err == nil {
					t.Fatal("canceled registry request succeeded")
				}
			case <-time.After(time.Second):
				t.Fatal("canceled registry request did not stop")
			}
		})
	}
}

func TestUnmodifiedKeelReadsAuthenticatedMultiarchRegistry(t *testing.T) {
	requests := 0
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		user, password, ok := r.BasicAuth()
		if !ok || user != "test" || password != "private-fixture" {
			w.Header().Set("WWW-Authenticate", `Basic realm="registry"`)
			w.WriteHeader(http.StatusUnauthorized)
			return
		}
		requests++
		w.Header().Set("Content-Type", "application/vnd.oci.image.index.v1+json")
		w.Header().Set("Docker-Content-Digest", newDigest)
		fmt.Fprintf(w, `{"schemaVersion":2,"mediaType":"application/vnd.oci.image.index.v1+json","manifests":[{"digest":%q,"platform":{"os":"linux","architecture":"amd64"}},{"digest":%q,"platform":{"os":"linux","architecture":"arm64"}}]}`, oldDigest, newDigest)
	}))
	defer server.Close()
	host := strings.TrimPrefix(server.URL, "http://")
	object := database(t, "private", "@hourly")
	_ = unstructured.SetNestedField(object.Object, host+"/postgresql:17-standard-trixie@"+oldDigest, "spec", "imageName")
	_ = unstructured.SetNestedSlice(object.Object, []interface{}{map[string]interface{}{"name": "pull"}}, "spec", "imagePullSecrets")
	selected, err := selected(object)
	if err != nil {
		t.Fatal(err)
	}
	core := kubefake.NewSimpleClientset(&corev1.Secret{
		ObjectMeta: metav1.ObjectMeta{Name: "pull", Namespace: "test"},
		Type:       corev1.SecretTypeDockerConfigJson,
		Data:       map[string][]byte{corev1.DockerConfigJsonKey: []byte(fmt.Sprintf(`{"auths":{%q:{"username":"test","password":"private-fixture"}}}`, host))},
	})
	client, err := newRegistry(context.Background(), selected, core.CoreV1(), map[string]bool{host: true})
	if err != nil {
		t.Fatal(err)
	}
	opts := registry.Opts{Name: "postgresql", Tag: "17-standard-trixie"}
	digest, err := client.Digest(opts)
	if err != nil || digest != newDigest {
		t.Fatalf("digest=%s error=%v", digest, err)
	}
	platforms, err := client.Platforms(opts)
	if err != nil || !types.PlatformsSupportAll(platforms, []types.Platform{{OS: "linux", Architecture: "amd64"}, {OS: "linux", Architecture: "arm64"}}) {
		t.Fatalf("platforms=%v error=%v", platforms, err)
	}
	if requests < 2 {
		t.Fatal("registry was not read")
	}
}
