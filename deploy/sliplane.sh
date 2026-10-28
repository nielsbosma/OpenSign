#!/usr/bin/env bash
# Deploys OpenSign to Sliplane through its API (https://ctrl.sliplane.io): a managed Postgres, an S3
# bucket, Gotenberg on the private network for Word files, and the app itself.
#
#   SLIPLANE_TOKEN=api_rw_org_... ./deploy/sliplane.sh            first install, or update to :latest
#   SLIPLANE_TOKEN=... OPENSIGN_TAG=0.2.0 ./deploy/sliplane.sh     update to a release
#
# The token needs read and write access to projects, servers, services, postgres and buckets, and
# secrets:read so the database password can be read once. Nothing is written to disk.
#
# Optional:
#   SLIPLANE_SERVER_ID   the server to run on (default: the only server, or the first one)
#   SLIPLANE_PROJECT     project name (default: OpenSign)
#   SLIPLANE_REGION      ger, fin, us-east, ... for the database and bucket (default: ger)
#   POSTGRES_INSTANCE    starter, base, ... (default: starter)
#   DATABASE_URL / S3_ENDPOINT S3_BUCKET S3_ACCESS_KEY S3_SECRET_KEY S3_REGION
#                        bring your own database or bucket instead of creating them
#   RESEND_API_KEY, RESEND_FROM   pin email to Resend (otherwise the setup wizard asks for it)
#   AI_PROVIDER, AI_MODEL, AI_API_KEY   pin the optional AI assistant
#
# Afterwards, open the printed URL and enter the setup code from the service's logs.

set -euo pipefail

API=https://ctrl.sliplane.io/v0
IMAGE=${OPENSIGN_IMAGE:-ghcr.io/nielsbosma/opensign}
TAG=${OPENSIGN_TAG:-latest}
PROJECT=${SLIPLANE_PROJECT:-OpenSign}
REGION=${SLIPLANE_REGION:-ger}
APP_NAME=opensign
CONVERTER_NAME=opensign-converter

for tool in curl jq openssl; do
  command -v "$tool" >/dev/null || { echo "This script needs $tool." >&2; exit 1; }
done
: "${SLIPLANE_TOKEN:?Set SLIPLANE_TOKEN to an API token from the Sliplane dashboard (team settings).}"

api() {
  local method=$1 path=$2 body=${3:-}
  local args=(-sS -X "$method" -H "Authorization: Bearer $SLIPLANE_TOKEN" -H "Accept: application/json" -w '\n%{http_code}')
  [[ -n "$body" ]] && args+=(-H "Content-Type: application/json" --data-binary @-)
  local out code
  out=$(printf '%s' "$body" | curl "${args[@]}" "$API$path")
  code=${out##*$'\n'}
  out=${out%$'\n'*}
  if [[ "$code" -ge 400 ]]; then
    echo "Sliplane refused $method $path ($code): $(jq -r '.message // .' <<<"$out" 2>/dev/null || echo "$out")" >&2
    exit 1
  fi
  printf '%s' "$out"
}

step() { printf '\n==> %s\n' "$*" >&2; }

step "Checking the token"
api GET /me >/dev/null && echo "The token works." >&2

step "Project $PROJECT"
project_id=$(api GET /projects | jq -r --arg n "$PROJECT" '[.[] | select(.name == $n)][0].id // empty')
if [[ -z "$project_id" ]]; then
  project_id=$(api POST /projects "$(jq -n --arg n "$PROJECT" '{name: $n}')" | jq -r .id)
  echo "Created $project_id" >&2
fi

services=$(api GET "/projects/$project_id/services")
app_id=$(jq -r --arg n "$APP_NAME" '[.[] | select(.name == $n)][0].id // empty' <<<"$services")
if [[ -n "$app_id" ]]; then
  step "Updating $APP_NAME to $IMAGE:$TAG"
  api POST "/projects/$project_id/services/$app_id/deploy" "$(jq -n --arg t "$TAG" '{tag: $t}')" >/dev/null
  domain=$(api GET "/projects/$project_id/services/$app_id" | jq -r '.network.managedDomain // empty')
  echo "Deploying. Migrations run on start." >&2
  echo "https://${domain:-<your domain>}"
  exit 0
fi

step "Server"
server_id=${SLIPLANE_SERVER_ID:-$(api GET /servers | jq -r '.[0].id // empty')}
[[ -n "$server_id" ]] || { echo "Create a server in Sliplane first, or set SLIPLANE_SERVER_ID." >&2; exit 1; }
echo "Using $server_id" >&2

if [[ -z "${DATABASE_URL:-}" ]]; then
  step "Postgres (${POSTGRES_INSTANCE:-starter}, $REGION)"
  pg_id=$(api POST /postgres "$(jq -n --arg r "$REGION" --arg i "${POSTGRES_INSTANCE:-starter}" \
    '{name: "OpenSign", instanceType: $i, region: $r, databaseName: "opensign", databaseUser: "opensign"}')" | jq -r .id)
  for _ in $(seq 1 90); do
    pg=$(api GET "/postgres/$pg_id")
    [[ $(jq -r .status <<<"$pg") == running ]] && break
    sleep 10
  done
  DATABASE_URL=$(jq -r '.connection.uri // empty' <<<"$pg")
  [[ -n "$DATABASE_URL" ]] || { echo "The database is not running yet, or the token lacks secrets:read. Re-run with DATABASE_URL set." >&2; exit 1; }
  case "$DATABASE_URL" in *sslmode=*) ;; *\?*) DATABASE_URL="$DATABASE_URL&sslmode=require" ;; *) DATABASE_URL="$DATABASE_URL?sslmode=require" ;; esac
fi

if [[ -z "${S3_BUCKET:-}" ]]; then
  step "Bucket ($REGION)"
  S3_BUCKET="opensign-$(openssl rand -hex 4)"
  bucket=$(api POST /buckets "$(jq -n --arg n "$S3_BUCKET" --arg r "$REGION" '{name: $n, region: $r, versioning: true}')")
  S3_ENDPOINT=$(jq -r .endpoint <<<"$bucket")
  key=$(api POST "/buckets/$(jq -r .id <<<"$bucket")/keys" '{"name": "OpenSign"}')
  S3_ACCESS_KEY=$(jq -r .accessKeyId <<<"$key")
  S3_SECRET_KEY=$(jq -r .secretAccessKey <<<"$key")
  S3_REGION=${S3_REGION:-$REGION}
fi

step "Document converter (Gotenberg, private)"
converter=$(jq -r --arg n "$CONVERTER_NAME" '[.[] | select(.name == $n)][0] // empty' <<<"$services")
if [[ -z "$converter" ]]; then
  converter=$(api POST "/projects/$project_id/services" "$(jq -n --arg s "$server_id" --arg n "$CONVERTER_NAME" '{
    name: $n, serverId: $s,
    deployment: {url: "docker.io/gotenberg/gotenberg:8"},
    cmd: "gotenberg --api-timeout=60s --chromium-disable-routes=true --webhook-disable=true --prometheus-disable-collect=true",
    network: {public: false},
    healthcheck: "/health",
    env: [{key: "PORT", value: "3000", secret: false}]
  }')")
fi
CONVERTER_URL="http://$(jq -r '.network.internalDomain' <<<"$converter"):3000"

step "OpenSign ($IMAGE:$TAG)"
env_json=$(jq -n \
  --arg db "$DATABASE_URL" --arg secret "$(openssl rand -base64 48 | tr -d '\n')" \
  --arg s3e "${S3_ENDPOINT:-}" --arg s3b "$S3_BUCKET" --arg s3a "$S3_ACCESS_KEY" --arg s3k "$S3_SECRET_KEY" --arg s3r "${S3_REGION:-us-east-1}" \
  --arg conv "$CONVERTER_URL" \
  '[{key: "DATABASE_URL", value: $db, secret: true},
    {key: "APP_SECRET", value: $secret, secret: true},
    {key: "S3_ENDPOINT", value: $s3e, secret: false},
    {key: "S3_BUCKET", value: $s3b, secret: false},
    {key: "S3_ACCESS_KEY", value: $s3a, secret: false},
    {key: "S3_SECRET_KEY", value: $s3k, secret: true},
    {key: "S3_REGION", value: $s3r, secret: false},
    {key: "CONVERTER_URL", value: $conv, secret: false},
    {key: "PORT", value: "3000", secret: false}]')
add_env() { env_json=$(jq --arg k "$1" --arg v "$2" --argjson s "$3" '. + [{key: $k, value: $v, secret: $s}]' <<<"$env_json"); }
[[ -n "${RESEND_API_KEY:-}" ]] && add_env OPENSIGN_EMAIL_API_KEY "$RESEND_API_KEY" true
[[ -n "${RESEND_FROM:-}" ]] && add_env OPENSIGN_EMAIL_FROM_ADDRESS "$RESEND_FROM" false
[[ -n "${AI_PROVIDER:-}" ]] && add_env OPENSIGN_AI_PROVIDER "$AI_PROVIDER" false
[[ -n "${AI_MODEL:-}" ]] && add_env OPENSIGN_AI_MODEL "$AI_MODEL" false
[[ -n "${AI_API_KEY:-}" ]] && add_env OPENSIGN_AI_API_KEY "$AI_API_KEY" true

app=$(api POST "/projects/$project_id/services" "$(jq -n --arg s "$server_id" --arg n "$APP_NAME" --arg img "$IMAGE:$TAG" --argjson env "$env_json" '{
  name: $n, serverId: $s,
  deployment: {url: $img},
  network: {public: true, protocol: "http"},
  healthcheck: "/api/health",
  env: $env
}')")
app_id=$(jq -r .id <<<"$app")
domain=$(jq -r '.network.managedDomain // empty' <<<"$app")
if [[ -n "$domain" ]]; then
  api PUT "/projects/$project_id/services/$app_id/env/APP_URL" "$(jq -n --arg v "https://$domain" '{value: $v, secret: false}')" >/dev/null
  api POST "/projects/$project_id/services/$app_id/deploy" '{}' >/dev/null
fi

cat >&2 <<NEXT

OpenSign is deploying. Next:
  1. Open https://${domain:-<the service domain>} once the deploy is green.
  2. Copy the setup code from the service's Logs in Sliplane and create the first admin.
  3. Enter your Resend API key and sender address; the wizard sends a test email.
  4. With a custom domain, set APP_URL to it in the service's environment and redeploy.
NEXT
echo "https://${domain:-}"
