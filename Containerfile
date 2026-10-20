# zeabur's image: one static binary (API and embedded web interface) and Mozilla's CA bundle (for
# Zeabur Identity's endpoints); no shell, no package manager. kube-apiserver is trusted through the service
# account's CA only. Runs as a non-root user.
# Built by scripts/build-image.sh.
FROM scratch
# Modes come from the build context (binary 0555, directories 0755, bundle 0444); see
# build-image.sh.
COPY zeabur /zeabur
COPY etc /etc
USER 65532:65532
ENTRYPOINT ["/zeabur"]
