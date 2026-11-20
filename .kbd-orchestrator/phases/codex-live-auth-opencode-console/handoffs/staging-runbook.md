# Staging deployment and rollback

Use this procedure after KBD accepts the SDK image and the owner authorizes staging deployment. The deployment host, Docker context, registry, Compose project, expected browser origin and rollback baseline must identify the actual staging environment. Fill in the deployment record from observed values and retain it with the release evidence.

## Preserve the tested image

On the build host, keep a local candidate tag pointing to the image ID recorded in the verification build receipt before fixture cleanup removes its temporary tag. Record its OS and architecture. The local image ID identifies the image configuration. The registry manifest has its own digest.

For a separately approved registry deployment, push the accepted image to the designated repository. Resolve the registry manifest for the staging platform and verify its configuration digest equals the image ID in the build receipt. Use that manifest digest for the deployment. If staging needs another platform, that platform's image needs its own verification.

## Live account evidence before acceptance

Obtain the account owner's explicit consent for a dedicated Codex profile, one genuine login, managed renewal, and the bounded ChatGPT Pro verification turn. Agree on the account, model and independent usage evidence before sending the turn. The operator completes the provider sign-in directly. Keep API billing credentials out of the subscription run.

1. Record the candidate image or native binary identity, proxy process identity, selected SDK and host versions, starting connection generation and independent account usage baseline. Use a pseudonym for the account in the report.
2. Leave the proxy running. Execute the supported login against the dedicated Codex home. Mount the whole directory when exercising the image. Measure the elapsed time from the store commit to the first successful authenticated observation, correlate the new generation with the same process, and retain only the timing, process identity and redacted result.
3. Exercise the managed renewal with the account owner's approved profile. Record the resulting freshness and the independent authenticated observation. If completion is uncertain, reconcile with the provider before retrying.
4. Submit the agreed turn from the certified OpenCode 2.x plugin through the Codex SDK. Correlate the native turn and its time window with independent evidence that the same subscriber's Pro allowance served it. If the account's usage view cannot attribute the turn, keep the Pro row blocked and retain the redacted observation.

Keep token files, authorization headers, device codes and full provider responses out of release evidence. Record disposition per scenario and obtain separate deployment consent after acceptance.

## Local OrbStack target

Use the following Bash commands from the repository root **after acceptance and separate owner approval for staging**. This first-install procedure targets Compose project `openai-proxy-staging` on Docker context `orbstack` at `http://127.0.0.1:18181`. It starts the console signed out. Four project-owned named volumes hold its dedicated Codex home, config, service data and workspace. The local operator can pair as administrator. The OpenCode driver requires a separately configured provider.

The overlay selects the tag retained for candidate `196a0ba3` at source `885a17a`. Confirm that acceptance identifies this exact image ID. If the accepted platform or digest changes, update the overlay, expected ID and smoke expectations before requesting deployment consent. The retained local image needs no registry. [Image receipt](../repairs/b5-6/retained-image.json) · [OrbStack overlay](staging-local.compose.yaml)

### Preflight and deployment record

This preflight reserves no resources. It checks the rendered configuration, image identity, existing Compose resources, and host port. Run the blocks below in the same Bash shell. If any preflight check fails, reconcile the observed state before starting a service. For an existing staging deployment, first record the running image ID, its retained or pullable reference, the previous overlay, and a compatible volume backup or rollback strategy.

```bash
set -euo pipefail
umask 077
handoff=.kbd-orchestrator/phases/codex-live-auth-opencode-console/handoffs
project=openai-proxy-staging
expected=sha256:b0558c5097cfd633eb1f25f1e35dc80dfc853262dadd0fbf44a0e8d089223886
tag=openai-proxy:kbd-196a0ba3f60b
engine=(docker --context orbstack)
compose=("${engine[@]}" compose --project-name "$project" --env-file /dev/null
  -f docker-compose.yaml -f "$handoff/staging-local.compose.yaml")
record=$(mktemp -d "${TMPDIR:-/tmp}/openai-proxy-staging.XXXXXXXX")
echo "Deployment record: $record"
"${compose[@]}" config --format json > "$record/compose.json"
test "$("${compose[@]}" config --images)" = "$tag"
test "$("${engine[@]}" image inspect --format '{{.Id}} {{.Os}}/{{.Architecture}}' "$tag")" = "$expected linux/arm64"
git rev-parse HEAD > "$record/config-revision"
printf '%s\n' "$expected" > "$record/expected-image"
"${compose[@]}" ps --all --quiet > "$record/previous-containers"
test ! -s "$record/previous-containers"
for volume in oproxy-codex oproxy-config oproxy-data oproxy-workspace; do
  existing=$("${engine[@]}" volume ls --filter "name=^${project}_${volume}$" --format '{{.Name}}')
  if test -n "$existing"; then
    echo "Existing staging volume requires inspection: $volume" >&2
    exit 1
  fi
done
python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",18181)); s.close()'
```

### Start and smoke-test the accepted image

```bash
"${compose[@]}" up --no-build --pull never --wait --wait-timeout 120 proxy
cid=$("${compose[@]}" ps -q proxy)
running=$("${engine[@]}" inspect --format '{{.Image}}' "$cid")
printf '%s\n' "$running" > "$record/running-image"
test "$running" = "$expected"
"${engine[@]}" inspect --format '{{json .Mounts}}' "$cid" > "$record/mounts.json"
"${compose[@]}" exec -T proxy cat /home/oproxy/.local/share/oproxy/console/pairing-token |
  python3 "$handoff/staging-smoke.py" > "$record/smoke.json"
cat "$record/smoke.json"
```

The smoke script uses the published loopback port. It checks health, the chat and settings entry points, a hashed JavaScript asset, denial of anonymous settings access, and the paired local administrator’s settings. The fresh credential home should report `signed_out`. The Codex SDK should report `installed`; the OpenCode SDK should report `scoped_provider_required` until the operator selects its provider. Pairing consumes the private code. The script logs out and verifies that the former session loses access, leaving a redacted JSON receipt. If the code expires between reading and pairing, read the fresh code and rerun the smoke command. Preserve the receipt and actual running digest with the phase evidence. [Smoke script](staging-smoke.py) · [Console guide](../../../../docs/console.md)

For a separately approved **shared** target, record its ingress, protected principal configuration, and dedicated owner mounts. Pair as the intended administrator, verify a chat principal receives HTTP 403 for administrator settings, and check each principal’s configured driver state. A controlled streaming and cancellation check needs a provisioned synthetic upstream. Subscriber use retains its separate approval.

### Roll back a first install

If startup, image comparison or smoke fails after creation, preserve the status and failed step in the deployment record. Use the same variables and configuration from the preflight. If the shell exited on failure, restore the printed record path with `record=/path/to/that/record` and repeat the preflight variable assignments through the `compose` array from the repository root. The first-install rollback removes only the project containers and network. Named volumes retain the deployment data. Verify that the project has stopped and the port is free.

```bash
"${compose[@]}" ps --all --format json > "$record/before-rollback.json"
"${compose[@]}" down
test -z "$("${compose[@]}" ps --all --quiet)"
python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",18181)); s.close()'
date -u '+%Y-%m-%dT%H:%M:%SZ' > "$record/rollback-complete"
```

For an upgrade, restore the recorded prior configuration and exact image in the same project, then compare the running image ID and repeat its ingress and settings smoke. Restore persistent data from the recorded backup if required. Publishing or pulling from a private registry applies only to a separately approved registry target. [Docker Compose overlays](https://docs.docker.com/compose/how-tos/multiple-compose-files/merge) · [Compose up options](https://docs.docker.com/reference/cli/docker/compose/up/)
