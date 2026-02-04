# OpenVoicy Shared Schemas

This directory contains JSON Schema definitions that serve as the **single source of truth** for types shared across Rust, TypeScript, and Python implementations.

## Schemas

| Schema | Description |
|--------|-------------|
| `ReplacementRule.schema.json` | Text replacement rule definition |
| `AppConfig.schema.json` | Application configuration |

## Usage

### Validation

```bash
# Validate a config file
python validate.py AppConfig.schema.json /path/to/config.json

# Validate a replacement rule
python validate.py ReplacementRule.schema.json /path/to/rule.json

# Run self-tests
python validate.py --self-test

# Validate test vectors
python validate.py --test-vectors
```

### In Python (Runtime)

```python
import jsonschema
import json

# Load schema
with open("shared/schema/ReplacementRule.schema.json") as f:
    schema = json.load(f)

# Validate
rule = {"id": "test", "enabled": True, ...}
jsonschema.validate(rule, schema)  # Raises on error
```

### In TypeScript (Compile-time)

Generate types from schema:

```bash
npx json-schema-to-typescript ReplacementRule.schema.json > src/types/ReplacementRule.ts
```

Or use a runtime validator like `ajv`:

```typescript
import Ajv from 'ajv';
import schema from '../shared/schema/ReplacementRule.schema.json';

const ajv = new Ajv();
const validate = ajv.compile(schema);

if (!validate(data)) {
  console.error(validate.errors);
}
```

### In Rust (Compile-time)

Use `schemars` to generate schemas from types, or `typify` to generate types from schemas:

```rust
// Generate schema from Rust type
use schemars::JsonSchema;

#[derive(JsonSchema)]
struct ReplacementRule { ... }

// Or generate Rust types from schema using typify crate
```

## Schema Design Principles

1. **Required fields are explicit** - All required fields are listed in the `required` array
2. **Defaults are documented** - Default values in `default` properties
3. **Constraints are enforced** - `minLength`, `maxLength`, `minimum`, `maximum` where applicable
4. **Examples included** - Real-world examples in `examples` array
5. **additionalProperties: false** - Strict validation rejects unknown fields

## Type Alignment Status

### ReplacementRule

| Field | Python | TypeScript | Rust | Schema |
|-------|--------|------------|------|--------|
| id | Required | - | - | Required |
| enabled | Required | Required | Required | Required |
| kind | Required | - | - | Required |
| pattern | Required | Required | Required | Required |
| replacement | Required | Required | Required | Required |
| word_boundary | Required | - | - | Required |
| case_sensitive | Required | - | - | Required |
| description | Optional | - | - | Optional |
| origin | Optional | - | - | Optional |

**Note:** The Rust (`src-tauri/src/config.rs`) and TypeScript (`src/types.ts`) ReplacementRule types are simplified. The full schema matches the Python implementation which is used for actual text processing.

### AppConfig

All implementations align on the top-level structure:
- schema_version
- audio (AudioConfig)
- hotkeys (HotkeyConfig)
- injection (InjectionConfig)
- model (ModelConfig) - optional
- replacements (ReplacementRule[])
- ui (UiConfig)
- presets (PresetsConfig)

## CI Integration

Add schema validation to CI:

```yaml
# .github/workflows/ci.yml
- name: Validate schemas
  run: |
    cd shared/schema
    python validate.py --self-test
    python validate.py --test-vectors
```

## Schema Drift Detection

To detect when implementations diverge from schemas:

1. Generate types from schema for TypeScript
2. Compare generated types with actual types
3. Run schema validation in tests

Future: Automated drift detection script comparing:
- Rust: serde struct definitions
- TypeScript: interface definitions
- Python: dataclass definitions

## Versioning

Schemas are versioned via the `schema_version` field in AppConfig:
- **v1** (current): Initial schema with focus_guard_enabled

When making breaking changes:
1. Increment schema_version
2. Add migration logic in each implementation
3. Update this README
