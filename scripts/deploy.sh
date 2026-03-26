#!/bin/bash
# Deploy Cthulu to Google Cloud Run
#
# Prerequisites:
#   - gcloud CLI installed and authenticated
#   - Docker installed
#   - GCP project set: gcloud config set project YOUR_PROJECT
#
# Usage:
#   ./scripts/deploy.sh                          # Deploy with defaults
#   ./scripts/deploy.sh --api-key sk-ant-...     # Set team API key
#   ANTHROPIC_API_KEY=sk-ant-... ./scripts/deploy.sh

set -euo pipefail

# Config
PROJECT_ID=$(gcloud config get-value project 2>/dev/null)
REGION="${CLOUD_RUN_REGION:-us-central1}"
SERVICE_NAME="${CLOUD_RUN_SERVICE:-cthulu}"
IMAGE="gcr.io/${PROJECT_ID}/${SERVICE_NAME}"

if [ -z "$PROJECT_ID" ]; then
  echo "Error: No GCP project set. Run: gcloud config set project YOUR_PROJECT"
  exit 1
fi

# Parse args
API_KEY="${ANTHROPIC_API_KEY:-}"
while [[ $# -gt 0 ]]; do
  case $1 in
    --api-key) API_KEY="$2"; shift 2 ;;
    --region) REGION="$2"; shift 2 ;;
    --service) SERVICE_NAME="$2"; shift 2 ;;
    *) echo "Unknown arg: $1"; exit 1 ;;
  esac
done

echo "=== Cthulu Cloud Deploy ==="
echo "  Project:  $PROJECT_ID"
echo "  Region:   $REGION"
echo "  Service:  $SERVICE_NAME"
echo "  API Key:  ${API_KEY:+set (${#API_KEY} chars)}${API_KEY:-not set (users must set their own)}"
echo ""

# Build
echo "Building Docker image..."
docker build -t "$IMAGE:latest" .

# Push
echo "Pushing to GCR..."
docker push "$IMAGE:latest"

# Deploy
echo "Deploying to Cloud Run..."
ENV_VARS="STORAGE=sqlite,AUTH_ENABLED=true"
if [ -n "$API_KEY" ]; then
  ENV_VARS="${ENV_VARS},ANTHROPIC_API_KEY=${API_KEY}"
fi

gcloud run deploy "$SERVICE_NAME" \
  --image "$IMAGE:latest" \
  --region "$REGION" \
  --port 8081 \
  --allow-unauthenticated \
  --set-env-vars "$ENV_VARS" \
  --memory 1Gi \
  --cpu 2 \
  --min-instances 0 \
  --max-instances 3 \
  --timeout 300

# Get URL
URL=$(gcloud run services describe "$SERVICE_NAME" --region "$REGION" --format 'value(status.url)')
echo ""
echo "=== Deployed! ==="
echo "  URL: $URL"
echo ""
echo "Next steps:"
echo "  1. Open $URL in your browser"
echo "  2. Sign up with your email"
if [ -z "$API_KEY" ]; then
  echo "  3. Set your Anthropic API key on the welcome screen"
else
  echo "  3. Team API key is configured — users can skip the key setup"
fi
echo ""
echo "To set a team API key later:"
echo "  gcloud run services update $SERVICE_NAME --region $REGION --set-env-vars ANTHROPIC_API_KEY=sk-ant-..."
