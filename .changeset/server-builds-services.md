---
default: major
---

# The server makes the objects of a new image or Git service

The "Docker image" form and a Compose import send one `createImageService` change per service, and "Deploy from Git" sends one `createGitService` change. The server makes the Secrets, claims, Deployment, Service, Ingress and Build from them, with the pod settings of the security profile, and checks the service name, the image, the replicas, the branch and the host. `POST /projects/{project}/image-service/yaml` gives the objects of an image service as YAML for the YAML editor. The `deployKey` change is gone: a Git service with `access.type` `deployKey` gets its key at apply. The public key is now read by `POST /projects/{project}/builds/{build}/deploy-key`, which replaces `…/deploy-keys/{name}/public`. The `ServiceReplicas` schema is gone; the limit is on `ImageServiceChange.replicas`. `UploadMethod` is renamed `BuildMethod`. The web interface builds every API address with the generated client.
