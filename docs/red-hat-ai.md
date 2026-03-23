# How to Use in Red Hat AI

This guide explains how to use inference-time scaling (ITS) within the Red Hat AI ecosystem — both through the officially supported product path and through upstream open-source options.

## How ITS is Available Today

| Path | How | Status |
|------|-----|--------|
| **Red Hat AI Python Index** | Install the its_hub SDK in an RHOAI workbench | Officially supported in the product |
| **Upstream SDK** | `pip install its_hub` from PyPI | Open-source, community-supported |
| **Upstream IaaS Gateway** | Deploy the FastAPI gateway | Open-source, community-supported |
| **Red Hat AI Gateway** | Integrated ITS through the managed gateway | Coming soon |

---

## Using ITS in Red Hat OpenShift AI (Product)

The officially supported way to use ITS today is through the **its_hub SDK**, available on the [Red Hat AI Python Index](https://access.redhat.com/articles/7137881). This gives you Red Hat-built packages with a secure supply chain, fully supported on Red Hat OpenShift AI.

### 1. Install in an RHOAI Workbench

RHOAI workbench base images come **pre-configured** to use the Red Hat AI Python Index. No extra index URLs are needed — just install:

```bash
# Core installation (Best-of-N, Self-Consistency)
pip install its_hub

# With Process Reward Model support (Particle Filtering, Beam Search)
pip install "its_hub[prm]"
```

The Red Hat AI Python Index currently provides its_hub versions 0.3.4 and 0.3.5.

### 2. Serve a Model with RHOAI

Red Hat OpenShift AI includes a vLLM serving runtime for hosting models on-cluster:

1. In the RHOAI dashboard, navigate to **Model Serving** > **Deploy Model**
2. Select the **vLLM ServingRuntime**
3. Deploy your model (e.g., `ibm-granite/granite-3.3-8b-instruct`, `meta-llama/Llama-3.2-3B-Instruct`)
4. Copy the **Inference endpoint** URL from the dashboard

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

### Using with Granite Models

IBM Granite models are available through RHOAI and work well with ITS:

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

### Example Endpoint Configurations

| Platform | Endpoint Format | API Key |
|----------|----------------|---------|
| RHOAI (in-cluster) | `http://<service-name>:8080/v1` | RHOAI serving token |
| RHOAI (external route) | `https://<route-host>/v1` | RHOAI serving token |
| RHEL AI (local) | `http://localhost:8100/v1` | `NO_API_KEY` |

### Installing Outside an RHOAI Workbench

If you need to install in a custom environment (not a pre-configured RHOAI base image), specify the index URL directly. The URL is variant-specific:

```bash
# CPU-only
pip install its_hub \
    --extra-index-url https://console.redhat.com/api/pypi/public-rhai/rhoai/3.2/cpu-ubi9/simple/

# NVIDIA GPU (CUDA 12.9)
pip install its_hub \
    --extra-index-url https://console.redhat.com/api/pypi/public-rhai/rhoai/3.2/cuda12.9-ubi9/simple/

# AMD GPU (ROCm 6.4)
pip install its_hub \
    --extra-index-url https://console.redhat.com/api/pypi/public-rhai/rhoai/3.2/rocm6.4-ubi9/simple/
```

---

## Upstream / Open-Source Options

The following options use the upstream open-source its_hub project. They are community-supported and not part of the Red Hat product.

### Upstream SDK (PyPI)

Install directly from PyPI for use outside the Red Hat ecosystem:

```bash
pip install its_hub
```

See the [Quick Start Guide](quick-start.md) for usage examples.

### Upstream IaaS Gateway

The its_hub library includes a FastAPI-based gateway that provides an OpenAI-compatible API with inference-time scaling built in. Any application that speaks the OpenAI chat completions format can use it — just point to the gateway and add a `budget` parameter.

```bash
its-iaas --host 0.0.0.0 --port 8108
```

This gateway can be deployed on OpenShift or any container platform. For full setup and configuration, see the [IaaS Service Guide](iaas-service.md).

**Example: Using the gateway with the OpenAI client**

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

## Coming Soon: Red Hat AI Gateway

ITS will be integrated into the **Red Hat AI Gateway**, providing inference-time scaling as a managed service without needing to install the SDK or deploy a separate gateway. Stay tuned for updates.

---

## Further Reading

- [Red Hat AI Python Index](https://access.redhat.com/articles/7137881) — Package index details and supported environments
- [Red Hat OpenShift AI Documentation](https://docs.redhat.com/en/documentation/red_hat_openshift_ai_self-managed/) — Platform setup and model serving
- [RHEL AI Documentation](https://docs.redhat.com/en/documentation/red_hat_enterprise_linux_ai/) — Local AI model serving on RHEL
- [vLLM Serving Runtime](https://docs.redhat.com/en/documentation/red_hat_openshift_ai_self-managed/2-latest/html/serving_models/serving-large-models_serving-large-models#configuring-a-vllm-model-serving-runtime_serving-large-models) — Configuring vLLM on OpenShift AI
- [IaaS Service Guide](iaas-service.md) — Upstream gateway configuration reference
