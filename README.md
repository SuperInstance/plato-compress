# plato-compress

> Compression for PLATO tile streams — run-length, delta, dictionary, quantize, Huffman

## What This Does

plato-compress provides multiple compression methods for reducing tile data size. Sensor data has patterns: temperatures change slowly (delta encoding is efficient), HVAC states repeat (run-length encoding compresses well), and floating point precision often isn't needed (quantization reduces size).

## The Key Idea

IoT sensors on constrained networks (ESP32 → coordinator) need to send less data. Different data patterns suit different compression: constant values compress perfectly with run-length encoding, slowly-changing values with delta encoding, and repeated values with dictionary encoding. plato-compress gives you all of them.

## Install

```bash
cargo add plato-compress
```

## Quick Start

```rust
use plato_compress::*;

// Run-length: great for repeating values
let data = vec![20.0, 20.0, 20.0, 21.0, 21.0];
let rle = run_length_encode(&data);  // [(20.0, 3), (21.0, 2)]
let decoded = run_length_decode(&rle);

// Delta: great for slowly changing values
let deltas = delta_encode(&[20.0, 20.1, 20.2, 20.15]);
// [20.0, 0.1, 0.1, -0.05] — small values compress well

// Quantize: reduce precision
let quantized = quantize(&[22.345678, 22.349999], 1); // [22.3, 22.3]
```

## API Reference

| Method | Encode | Decode | Best For |
|---|---|---|---|
| Run-Length | `run_length_encode(data)` | `run_length_decode(pairs)` | Repeating/constant values |
| Delta | `delta_encode(data)` | `delta_decode(deltas)` | Slowly changing values |
| Dictionary | `dictionary_encode(data)` | `dictionary_decode(dict, indices)` | Limited vocabulary |
| Quantize | `quantize(data, precision)` | — | Reducing floating-point precision |
| Huffman | `huffman_encode(data)` | `huffman_decode(encoded)` | General purpose |

| Type | Description |
|---|---|
| `CompressionMethod` | `RunLength` / `Delta` / `Dictionary` / `Quantize` / `Huffman` |
| `CompressionConfig { method, lossy, precision }` | Configuration |
| `CompressionResult { original_size, compressed_size, ratio }` | Compression stats |

## Testing

20 tests: run-length encode/decode, delta encode/decode, dictionary encoding, quantization, Huffman coding, compression ratio, round-trip fidelity.

## License

Apache-2.0
