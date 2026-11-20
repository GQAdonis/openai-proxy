# Staging deployment and rollback

Use this procedure after KBD accepts the SDK image and the owner authorizes staging deployment. The deployment host, Docker context, registry, Compose project, expected browser origin and rollback baseline must identify the actual staging environment. Fill in the deployment record from observed values and retain it with the release evidence.

## Record before the change

- Accepted source commit, KBD candidate digest and registry image reference in the form `registry/repository@sha256:…`
- Staging host and Docker context, Compose project, configuration revision, and exact HTTPS origin or approved loopback origin
- Current container image ID, its pullable previous digest, and the location of the previous deployment configuration
- Backup identifier or verified compatibility of the previous image with the persistent service data and credential stores

Use the whole dedicated Codex home directory and a separate workspace and data volume. In shared mode, provide the protected principal configuration and existing per-owner homes. Set the browser origin to the real staging origin. Retain configuration references and digests in the record. Keep credentials in the existing secret store. [Console operator guide](../../../../docs/console.md)

## Deploy by digest

For a Compose staging target, provide a staging overlay that sets `services.proxy.image` to the accepted immutable reference. Keep the target, configuration and project explicit in every command. Inspect the resolved image before pulling. Use the same overlay for the pull and startup.

```sh
docker --context "$STAGING_DOCKER_CONTEXT" compose -p "$STAGING_COMPOSE_PROJECT" \
  -f docker-compose.yaml -f "$STAGING_RELEASE_OVERLAY" config --images
docker --context "$STAGING_DOCKER_CONTEXT" compose -p "$STAGING_COMPOSE_PROJECT" \
  -f docker-compose.yaml -f "$STAGING_RELEASE_OVERLAY" pull proxy
docker --context "$STAGING_DOCKER_CONTEXT" compose -p "$STAGING_COMPOSE_PROJECT" \
  -f docker-compose.yaml -f "$STAGING_RELEASE_OVERLAY" up --no-build --pull never --wait --wait-timeout 120 proxy
```

Compare the running container's image ID with the locally resolved image ID for the accepted registry digest. The Compose health check exercises the container listener. Record a separate request to the staging origin.

## Staging smoke

| Check | Record |
| --- | --- |
| External ingress and health | Request the real staging `/health` and record the successful response and observed origin. |
| Embedded console | Load `/ui/` and `/ui/settings` through the real origin. Verify the index and a content-addressed asset load. |
| Authentication and settings | Pair with a staging principal, verify its expected mode and role, and confirm administrator operations are denied to a chat principal in shared mode. Record returned statuses and observed state. |
| SDK availability | Confirm the declared Codex and OpenCode drivers report expected availability for the staging configuration. Use a controlled synthetic upstream for a streaming turn and cancellation if provisioned. Account-backed turns require the separately approved account and operation. |
| Persistence and rollback readiness | Verify the expected mounts and previous digest are available. Record the actual service image, smoke outcomes, and the operator who can perform rollback. |

## Roll back if startup or smoke fails

Save redacted container status and logs and the failing ingress response. Restore the previous staging overlay with the previously recorded digest. Repeat the pull and `up --no-build --pull never --wait` commands with that overlay, then compare the running image ID and repeat the ingress, console and session checks. Keep named volumes and credential directories mounted. If the rollback requires a data restore, use the recorded backup and the staging owner's restore procedure. On a first deployment with no previous service, stop the new service and restore the previous ingress configuration while retaining its volumes for inspection.

[Docker Compose file overlays](https://docs.docker.com/compose/how-tos/multiple-compose-files/merge) · [Compose up flags](https://docs.docker.com/reference/cli/docker/compose/up/)
