# Multifrost JavaScript

JavaScript/TypeScript implementation of Multifrost IPC library.

## Installation

```bash
npm install multifrost
```

Or for development:

```bash
cd javascript
npm install
```

## Quick Start

```typescript
import { ParentWorker } from 'multifrost';

const worker = new ParentWorker('./worker.ts');
await worker.start();

const result = await worker.proxy.add(2, 3);

await worker.close();
```

## API

See [API Reference](../docs/api-reference.md) for detailed documentation.

## Examples

See [examples/javascript](../examples/javascript/) for more examples.
