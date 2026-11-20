# Staging deployment and rollback

Use this procedure after KBD accepts the SDK image and the owner authorizes staging deployment. The deployment host, Docker context, registry, Compose project, expected browser origin and rollback baseline must identify the actual staging environment. Fill in the deployment record from observed values and retain it with the release evidence.

## Preserve the tested image

On the build host, keep a local candidate tag pointing to the image ID recorded in the verification build receipt before fixture cleanup removes its temporary tag. Record its OS and architecture. The local image ID identifies the image configuration. The registry manifest has its own digest.

After acceptance and authorization to publish to the designated staging registry, push the retained image. Resolve the registry manifest for the staging platform and verify its configuration digest equals the image ID in the build receipt. Use that manifest digest for the deployment. If staging needs another platform, that platform's image needs its own verification.

## Live account evidence before acceptance

Obtain the account owner's explicit consent for a dedicated Codex profile, one genuine login, managed renewal, and the bounded ChatGPT Pro verification turn. Agree on the account, model and independent usage evidence before sending the turn. The operator completes the provider sign-in directly. Keep API billing credentials out of the subscription run.

1. Record the candidate image or native binary identity, proxy process identity, selected SDK and host versions, starting connection generation and independent account usage baseline. Use a pseudonym for the account in the report.
2. Leave the proxy running. Execute the supported login against the dedicated Codex home. Mount the whole directory when exercising the image. Measure the elapsed time from the store commit to the first successful authenticated observation, correlate the new generation with the same process, and retain only the timing, process identity and redacted result.
3. Exercise the managed renewal with the account owner's approved profile. Record the resulting freshness and the independent authenticated observation. If completion is uncertain, reconcile with the provider before retrying.
4. Submit the agreed turn from the certified OpenCode 2.x plugin through the Codex SDK. Correlate the native turn and its time window with independent evidence that the same subscriber's Pro allowance served it. If the account's usage view cannot attribute the turn, keep the Pro row blocked and retain the redacted observation.

Keep token files, authorization headers, device codes and full provider responses out of release evidence. Record disposition per scenario and obtain separate deployment consent after acceptance.

## Record before the change

The owner selected the local `orbstack` Docker context. For this target the proposed Compose project is `openai-proxy-staging` with dedicated project volumes and browser origin `http://127.0.0.1:18181`. The pre-deployment inventory found no containers for this project and no listener on port 18181. Repeat both checks immediately before deployment. Do not substitute an existing service bound to a different port.

On this same engine a private registry is optional. Use the retained, candidate-specific local image tag in the Compose overlay, compare its full image ID with the accepted build receipt, and run Compose with `--no-build --pull never`. Set `PORT=18181` and `PROXY_CONSOLE_ORIGIN=http://127.0.0.1:18181`. Compare the created container's `.Image` with the receipt and smoke-test that exact origin. For a first deployment that fails smoke, stop and remove only this project's containers, preserving its volumes. Recheck port availability and record the observed state.

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
