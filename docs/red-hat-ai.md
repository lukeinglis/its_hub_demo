# How to Use in Red Hat AI

This guide explains how to use **its_hub** within the Red Hat AI ecosystem, covering installation from the Red Hat AI Python Index, deployment on OpenShift, and integration with Red Hat AI model serving.

## Overview

its_hub integrates with the Red Hat AI stack at multiple levels:

```
                        Red Hat AI Ecosystem
┌─────────────────────────────────────────────────────────┐
│                                                         │
│  ┌──────────────┐    ┌──────────────┐    ┌───────────┐  │
│  │  RHOAI       │    │  ITS Hub     │    │  vLLM     │  │
│  │  Workbench   │───►│  (SDK or     │───►│  Serving  │  │
│  │              │    │   Gateway)   │    │  Runtime  │  │
│  └──────────────┘    └──────────────┘    └───────────┘  │
│                                                         │
│  Red Hat OpenShift AI  /  RHEL AI                       │
└─────────────────────────────────────────────────────────┘
```

| Component | Role |
|-----------|------|
| **Red Hat OpenShift AI (RHOAI)** | ML platform for model serving, workbenches, and pipelines |
| **RHEL AI** | RHEL-based environment for local AI model serving and fine-tuning |
| **vLLM Serving Runtime** | High-performance model serving on OpenShift (included in RHOAI) |
| **its_hub SDK** | Python library imported directly into your application code |
| **its_hub IaaS Gateway** | FastAPI service providing an OpenAI-compatible API with inference-time scaling |

---

## Install from the Red Hat AI Python Index

The [Red Hat AI Python Index](https://access.redhat.com/articles/7137881) provides Red Hat-built Python packages with a secure supply chain, supported on Red Hat OpenShift AI environments.

### In an RHOAI Workbench

From a terminal in your RHOAI workbench (UBI9 base image):

```bash
# Core installation (Best-of-N, Self-Consistency, cloud APIs)
pip install its_hub \
    --extra-index-url https://download.devel.redhat.com/rel-eng/ai-python-index/

# With Process Reward Model support (Particle Filtering, Beam Search)
pip install "its_hub[prm]" \
    --extra-index-url https://download.devel.redhat.com/rel-eng/ai-python-index/
```

### In a RHEL AI Environment

```bash
pip install its_hub \
    --extra-index-url https://download.devel.redhat.com/rel-eng/ai-python-index/
```

### Supported Variants

The Red Hat AI Python Index provides packages for multiple platform targets:

| Variant | Use Case |
|---------|----------|
| `cpu-ubi9` | CPU-only inference, cloud API usage |
| `cuda12.9-ubi9` | NVIDIA GPU acceleration |
| `rocm6.4-ubi9` | AMD GPU acceleration |

### Verify Installation

```python
from its_hub.algorithms import BestOfN, SelfConsistency
from its_hub.lms import OpenAICompatibleLanguageModel
print("its_hub installed successfully")
```

---

## Deploy as an IaaS Gateway on OpenShift

The its_hub IaaS service can be deployed as a standalone gateway on OpenShift, providing an OpenAI-compatible API with inference-time scaling built in. Any application that speaks the OpenAI chat completions format can use it without code changes — just point to the gateway and add a `budget` parameter.

### Build the Container Image

Use the included devcontainer as a starting point:

```bash
# From the repo root
podman build -t its-iaas:latest -f .devcontainer/Dockerfile .
```

Or create a minimal production Dockerfile:

```dockerfile
FROM registry.redhat.io/ubi9/python-311:latest

WORKDIR /app
RUN pip install its_hub --extra-index-url https://download.devel.redhat.com/rel-eng/ai-python-index/

EXPOSE 8108
CMD ["its-iaas", "--host", "0.0.0.0", "--port", "8108"]
```

### Deploy on OpenShift

```yaml
# its-iaas-deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: its-iaas
spec:
  replicas: 1
  selector:
    matchLabels:
      app: its-iaas
  template:
    metadata:
      labels:
        app: its-iaas
    spec:
      containers:
        - name: its-iaas
          image: its-iaas:latest
          ports:
            - containerPort: 8108
          env:
            - name: OPENAI_API_KEY
              valueFrom:
                secretKeyRef:
                  name: its-iaas-secrets
                  key: openai-api-key
---
apiVersion: v1
kind: Service
metadata:
  name: its-iaas
spec:
  selector:
    app: its-iaas
  ports:
    - port: 8108
      targetPort: 8108
---
apiVersion: route.openshift.io/v1
kind: Route
metadata:
  name: its-iaas
spec:
  to:
    kind: Service
    name: its-iaas
  port:
    targetPort: 8108
  tls:
    termination: edge
```

```bash
# Create secrets and deploy
oc create secret generic its-iaas-secrets \
    --from-literal=openai-api-key=sk-...

oc apply -f its-iaas-deployment.yaml
```

### Configure the Gateway

Once deployed, configure the algorithm via the `/configure` endpoint:

```bash
ITS_ROUTE=$(oc get route its-iaas -o jsonpath='{.spec.host}')

curl -X POST "https://${ITS_ROUTE}/configure" \
  -H "Content-Type: application/json" \
  -d '{
    "provider": "litellm",
    "endpoint": "auto",
    "api_key": "your-api-key",
    "model": "gpt-4o-mini",
    "alg": "self-consistency",
    "tool_vote": "tool_hierarchical"
  }'
```

For full configuration options, see the [IaaS Service Guide](iaas-service.md).

---

## Use with Self-Hosted Models on OpenShift (vLLM Serving)

Red Hat OpenShift AI includes a vLLM serving runtime for hosting models on-cluster. its_hub works directly with these model serving endpoints.

### Set Up Model Serving in RHOAI

1. In the RHOAI dashboard, create a **Single-Model Serving** instance
2. Select the **vLLM** serving runtime
3. Deploy your model (e.g., `ibm-granite/granite-3.3-8b-instruct`, `meta-llama/Llama-3.2-3B-Instruct`)
4. Note the **Inference endpoint** URL from the dashboard

### Connect its_hub to the Served Model

```python
from its_hub.lms import OpenAICompatibleLanguageModel
from its_hub.algorithms import SelfConsistency

# Point to your RHOAI model serving endpoint
lm = OpenAICompatibleLanguageModel(
    endpoint="https://<model-serving-route>/v1",  # From RHOAI dashboard
    api_key="<serving-token>",                     # RHOAI auth token
    model_name="ibm-granite/granite-3.3-8b-instruct",
)

# Use Self-Consistency for reliable tool calling
sc = SelfConsistency(tool_vote="tool_hierarchical")
result = sc.infer(
    lm,
    "What is the capital of France?",
    budget=5,
)
print(result)
```

### Example Endpoint Configurations

| Platform | Endpoint Format | API Key |
|----------|----------------|---------|
| RHOAI (in-cluster) | `http://<service-name>:8080/v1` | RHOAI serving token |
| RHOAI (external route) | `https://<route-host>/v1` | RHOAI serving token |
| RHEL AI (local) | `http://localhost:8100/v1` | `NO_API_KEY` |
| vLLM standalone | `http://<host>:8100/v1` | `NO_API_KEY` or custom |

### Using with Granite Models

IBM Granite models are available through RHOAI and work well with its_hub:

```python
from its_hub.lms import OpenAICompatibleLanguageModel
from its_hub.algorithms import BestOfN
from its_hub.integration.reward_hub import LLMJudgeRewardModel

# Granite model served via RHOAI
lm = OpenAICompatibleLanguageModel(
    endpoint="https://granite-serving.apps.cluster.example.com/v1",
    api_key="<serving-token>",
    model_name="ibm-granite/granite-3.3-8b-instruct",
)

# Use Best-of-N with the same model as judge
judge = LLMJudgeRewardModel(
    model="ibm-granite/granite-3.3-8b-instruct",
    base_url="https://granite-serving.apps.cluster.example.com/v1",
    criterion="overall_quality",
    judge_type="groupwise",
    api_key="<serving-token>",
)

scaling_alg = BestOfN(judge)
result = scaling_alg.infer(lm, "Explain inference-time scaling", budget=4)
print(result)
```

---

## End-to-End Example on RHOAI

This walkthrough covers the full workflow: serving a model, installing its_hub, and running inference-time scaling — all on Red Hat OpenShift AI.

### 1. Serve a Model

In the RHOAI dashboard:
- Navigate to **Model Serving** > **Deploy Model**
- Select **vLLM ServingRuntime**
- Choose a model (e.g., `meta-llama/Llama-3.2-3B-Instruct`)
- Set resource limits and deploy
- Copy the inference endpoint URL once the model is ready

### 2. Install its_hub in a Workbench

Open a terminal in your RHOAI workbench:

```bash
pip install its_hub \
    --extra-index-url https://download.devel.redhat.com/rel-eng/ai-python-index/
```

### 3. Run Inference-Time Scaling

```python
from its_hub.lms import OpenAICompatibleLanguageModel
from its_hub.algorithms import SelfConsistency

# Connect to your RHOAI-served model
lm = OpenAICompatibleLanguageModel(
    endpoint="https://llama-serving.apps.cluster.example.com/v1",
    api_key="<your-rhoai-token>",
    model_name="meta-llama/Llama-3.2-3B-Instruct",
)

# Self-Consistency: generate 5 responses, vote on the best answer
sc = SelfConsistency()
result = sc.infer(lm, "What is 15% of 240?", budget=5)
print(f"Answer: {result}")
```

### 4. (Optional) Deploy as Gateway

To make inference-time scaling available to other services on the cluster, deploy the IaaS gateway as described [above](#deploy-as-an-iaas-gateway-on-openshift). Other applications can then use the standard OpenAI client:

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://its-iaas:8108/v1",
    api_key="dummy-key"
)

response = client.chat.completions.create(
    model="meta-llama/Llama-3.2-3B-Instruct",
    messages=[{"role": "user", "content": "What is 15% of 240?"}],
    extra_body={"budget": 5}
)
print(response.choices[0].message.content)
```

---

## Further Reading

- [Red Hat AI Python Index](https://access.redhat.com/articles/7137881) — Package index details and supported environments
- [Red Hat OpenShift AI Documentation](https://docs.redhat.com/en/documentation/red_hat_openshift_ai_self-managed/) — Platform setup and model serving
- [RHEL AI Documentation](https://docs.redhat.com/en/documentation/red_hat_enterprise_linux_ai/) — Local AI model serving on RHEL
- [vLLM Serving Runtime](https://docs.redhat.com/en/documentation/red_hat_openshift_ai_self-managed/2-latest/html/serving_models/serving-large-models_serving-large-models#configuring-a-vllm-model-serving-runtime_serving-large-models) — Configuring vLLM on OpenShift AI
- [IaaS Service Guide](iaas-service.md) — Full its_hub gateway configuration reference
