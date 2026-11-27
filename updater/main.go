// Zeabur's CloudNativePG updater. The dashboard configures resources as the person; this
// separately permissioned process performs opted-in scheduled updates when no one is signed in.
package main

import (
	"context"
	"flag"
	"fmt"
	"log/slog"
	"net/http"
	"os"
	"os/signal"
	"strings"
	"syscall"
	"time"

	"github.com/keel-hq/keel/registry"
	"github.com/sirupsen/logrus"
	k8stypes "k8s.io/apimachinery/pkg/types"
	"k8s.io/client-go/dynamic"
	coreclient "k8s.io/client-go/kubernetes/typed/core/v1"
	"k8s.io/client-go/rest"
)

func main() {
	if err := run(); err != nil {
		slog.Error("database updater stopped", "error", err)
		os.Exit(1)
	}
}

func run() error {
	var httpRegistries string
	var interval time.Duration
	flag.StringVar(&httpRegistries, "http-registries", "", "comma-separated registry hosts explicitly allowed over HTTP (development only)")
	flag.DurationVar(&interval, "scan-interval", 15*time.Second, "how often database settings and due schedules are read")
	flag.Parse()
	if interval < time.Second {
		return fmt.Errorf("scan interval must be at least one second")
	}
	config, err := rest.InClusterConfig()
	if err != nil {
		return err
	}
	config.Timeout = 15 * time.Second
	config.UserAgent = "zeabur-database-updater"
	api, err := dynamic.NewForConfig(config)
	if err != nil {
		return err
	}
	core, err := coreclient.NewForConfig(config)
	if err != nil {
		return err
	}
	plain := map[string]bool{}
	for _, host := range strings.Split(httpRegistries, ",") {
		if host = strings.TrimSpace(host); host != "" {
			if strings.ContainsAny(host, "/@") {
				return fmt.Errorf("HTTP registry entries must be host[:port], without a URL or credentials")
			}
			plain[host] = true
		}
	}
	c := &controller{api: api, core: core, plain: plain, jobs: map[k8stypes.UID]scheduled{}}
	c.registry = func(ctx context.Context, selected *target) (registry.Client, error) {
		return newRegistry(ctx, selected, c.core, c.plain)
	}
	logrus.SetLevel(logrus.WarnLevel)
	ctx, stop := signal.NotifyContext(context.Background(), syscall.SIGTERM, syscall.SIGINT)
	defer stop()
	mux := http.NewServeMux()
	mux.HandleFunc("/healthz", func(w http.ResponseWriter, _ *http.Request) { w.WriteHeader(http.StatusOK) })
	mux.HandleFunc("/readyz", func(w http.ResponseWriter, _ *http.Request) {
		if !c.ready.Load() {
			http.Error(w, "CloudNativePG discovery is not ready", http.StatusServiceUnavailable)
			return
		}
		w.WriteHeader(http.StatusOK)
	})
	server := &http.Server{Addr: ":9090", Handler: mux, ReadHeaderTimeout: 5 * time.Second}
	failed := make(chan error, 1)
	go func() { failed <- server.ListenAndServe() }()
	defer func() {
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		_ = server.Shutdown(ctx)
	}()
	ticker := time.NewTicker(interval)
	defer ticker.Stop()
	slog.Info("database updater started", "keel", "0.22.4 (bc640168f020)")
	for {
		if err := c.scan(ctx, time.Now()); err != nil && ctx.Err() == nil {
			slog.Error("cannot read database update settings", "error", err)
		}
		select {
		case <-ctx.Done():
			return nil
		case err := <-failed:
			return err
		case <-ticker.C:
		}
	}
}
