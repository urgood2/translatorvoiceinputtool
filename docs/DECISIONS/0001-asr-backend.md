# ADR 0001: ASR Backend Selection

**Status:** Accepted
**Date:** 2026-02-04
**Deciders:** Project Team

## Context

We need to select an automatic speech recognition (ASR) backend for the voice input tool. The selected model must:

1. Support high-quality English transcription
2. Work offline (no API calls required)
3. Have a permissive license allowing redistribution
4. Be efficient enough to run on consumer hardware
5. Provide punctuation and capitalization

## Considered Options

### Option 1: OpenAI Whisper (whisper-large-v3-turbo)

**Pros:**
- Excellent transcription quality
- Well-documented and widely used
- Multiple size variants available

**Cons:**
- Larger models (>1GB) for best quality
- MIT license is permissive but less explicit about model weights
- Slower inference on CPU

### Option 2: NVIDIA Parakeet TDT 0.6B v3

**Pros:**
- Excellent transcription quality (comparable to Whisper Large)
- Explicit CC-BY-4.0 license with clear redistribution rights
- 600M parameters (smaller than Whisper Large)
- Optimized for throughput with TDT architecture
- Automatic punctuation and capitalization
- Supports 25 European languages with auto-detection
- Word-level timestamps included

**Cons:**
- Newer model with smaller community
- Requires NeMo framework
- 2.5GB download for full model

### Option 3: Vosk (various models)

**Pros:**
- Very lightweight models available
- Apache 2.0 license
- Low resource requirements

**Cons:**
- Lower quality than Whisper/Parakeet
- No automatic punctuation
- Limited language support

## Decision

**Selected: NVIDIA Parakeet TDT 0.6B v3**

### Rationale

1. **License Clarity:** CC-BY-4.0 provides explicit, well-understood redistribution rights. We can confidently bundle and distribute the model with proper attribution.

2. **Quality vs Size:** The 600M parameter model provides transcription quality comparable to larger models while being more efficient. The TDT (Token Duration Transducer) architecture is optimized for fast inference.

3. **Features:** Built-in punctuation, capitalization, and timestamps eliminate the need for post-processing pipelines.

4. **Multilingual Support:** The 25-language support with automatic detection provides future expansion capability without model swapping.

5. **Commercial Viability:** CC-BY-4.0 explicitly allows commercial use, making this suitable for any deployment scenario.

## Fallback Strategy

If Parakeet becomes unavailable or licensing changes, the fallback is **OpenAI Whisper (small or base model)**:

- **Whisper Small**: 460MB, good accuracy, fully MIT licensed
- **Whisper Base**: 140MB, acceptable accuracy for basic dictation

The sidecar architecture abstracts the ASR backend through a common interface (`asr.transcribe`), allowing model swapping without API changes. To switch:

1. Update `MODEL_MANIFEST.json` with Whisper model details
2. Implement Whisper adapter in sidecar's ASR module
3. No changes needed to Rust core or IPC protocol

## Consequences

### Positive

- Clear legal standing for distribution
- Single model file simplifies deployment
- Rich feature set (punctuation, timestamps) out of the box
- Good inference performance on modern hardware

### Negative

- Requires NeMo toolkit or compatible runtime
- 2.5GB download on first use
- Less community resources compared to Whisper

### Mitigation

- Document NeMo integration clearly
- Implement robust download with progress feedback
- Monitor NVIDIA's model updates for improvements

## Performance Characteristics

| Metric | Parakeet TDT 0.6B | Whisper Small | Whisper Base |
|--------|-------------------|---------------|--------------|
| Model Size | ~2.5GB | ~460MB | ~140MB |
| Parameters | 600M | 244M | 74M |
| CPU Latency (10s audio) | ~2-4s | ~4-8s | ~2-4s |
| GPU Latency (10s audio) | <1s | ~1-2s | <1s |
| Memory (CPU) | ~4GB | ~2GB | ~1GB |
| Memory (GPU) | ~2GB | ~1GB | ~512MB |
| WER (English) | ~5% | ~7% | ~10% |

**Notes:**
- Latency measured on typical consumer hardware (8-core CPU, 8GB RAM)
- GPU measurements on NVIDIA RTX 3060 or equivalent
- WER (Word Error Rate) approximate; varies by accent/domain
- CPU-first baseline is the MVP requirement; GPU is optional optimization

## Implementation Notes

- Model ID in manifest: `parakeet-tdt-0.6b-v3`
- Source: `nvidia/parakeet-tdt-0.6b-v3` on HuggingFace
- Pinned revision: `6d590f77001d318fb17a0b5bf7ee329a91b52598`
- License: CC-BY-4.0 (attribution required)

## Related Documents

- [MODEL_MANIFEST.json](../../shared/model/MODEL_MANIFEST.json)
- [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md)
- [IPC_PROTOCOL_V1.md](../../shared/ipc/IPC_PROTOCOL_V1.md)

## References

- [NVIDIA Parakeet Model Card](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3)
- [CC-BY-4.0 License](https://creativecommons.org/licenses/by/4.0/)
- [NeMo Toolkit](https://github.com/NVIDIA/NeMo)
