use serde::{Deserialize, Serialize};

// ── Core types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompressionMethod {
    RunLength,
    Delta,
    Dictionary,
    Quantize,
    Huffman,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    pub method: CompressionMethod,
    pub lossy: bool,
    pub precision: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionResult {
    pub original_size: usize,
    pub compressed_size: usize,
    pub ratio: f64,
    pub method: CompressionMethod,
}

// ── Run-Length Encoding ─────────────────────────────────────────────────────

pub fn run_length_encode(data: &[f64]) -> Vec<(f64, usize)> {
    if data.is_empty() {
        return Vec::new();
    }
    let mut result = Vec::new();
    let mut current = data[0];
    let mut count: usize = 1;
    for &val in &data[1..] {
        if val == current {
            count += 1;
        } else {
            result.push((current, count));
            current = val;
            count = 1;
        }
    }
    result.push((current, count));
    result
}

pub fn run_length_decode(pairs: &[(f64, usize)]) -> Vec<f64> {
    pairs
        .iter()
        .flat_map(|&(val, count)| std::iter::repeat(val).take(count))
        .collect()
}

// ── Delta Encoding ──────────────────────────────────────────────────────────

pub fn delta_encode(data: &[f64]) -> Vec<f64> {
    if data.is_empty() {
        return Vec::new();
    }
    let mut result = Vec::with_capacity(data.len());
    result.push(data[0]);
    for i in 1..data.len() {
        result.push(data[i] - data[i - 1]);
    }
    result
}

pub fn delta_decode(deltas: &[f64]) -> Vec<f64> {
    if deltas.is_empty() {
        return Vec::new();
    }
    let mut result = Vec::with_capacity(deltas.len());
    result.push(deltas[0]);
    for i in 1..deltas.len() {
        result.push(result[i - 1] + deltas[i]);
    }
    result
}

// ── Quantization ────────────────────────────────────────────────────────────

pub fn quantize(data: &[f64], levels: usize) -> Vec<u8> {
    if data.is_empty() || levels == 0 {
        return Vec::new();
    }
    let min = data.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    if min == max {
        return vec![0; data.len()];
    }
    let step = (max - min) / levels as f64;
    data.iter()
        .map(|&v| {
            let idx = ((v - min) / step).floor() as usize;
            (idx.min(levels - 1)) as u8
        })
        .collect()
}

pub fn dequantize(indices: &[u8], levels: usize, original_min: f64, original_max: f64) -> Vec<f64> {
    if indices.is_empty() || levels == 0 {
        return Vec::new();
    }
    if original_min == original_max {
        return vec![original_min; indices.len()];
    }
    let step = (original_max - original_min) / levels as f64;
    indices
        .iter()
        .map(|&i| original_min + (i as f64 + 0.5) * step)
        .collect()
}

// ── Dictionary Encoding ─────────────────────────────────────────────────────

pub fn dictionary_encode(data: &[String]) -> (Vec<usize>, Vec<String>) {
    let mut dictionary: Vec<String> = Vec::new();
    let mut indices: Vec<usize> = Vec::new();
    for s in data {
        if let Some(pos) = dictionary.iter().position(|d| d == s) {
            indices.push(pos);
        } else {
            let idx = dictionary.len();
            dictionary.push(s.clone());
            indices.push(idx);
        }
    }
    (indices, dictionary)
}

pub fn dictionary_decode(indices: &[usize], dictionary: &[String]) -> Vec<String> {
    indices
        .iter()
        .map(|&i| dictionary[i].clone())
        .collect()
}

// ── Tile compress / decompress ──────────────────────────────────────────────

pub fn compress_tiles(values: &[f64], config: &CompressionConfig) -> Vec<u8> {
    if values.is_empty() {
        return Vec::new();
    }
    match config.method {
        CompressionMethod::RunLength => {
            let pairs = run_length_encode(values);
            let mut bytes = Vec::new();
            // header: number of pairs (u32 LE)
            bytes.extend_from_slice(&(pairs.len() as u32).to_le_bytes());
            for (val, count) in &pairs {
                bytes.extend_from_slice(&val.to_le_bytes());
                bytes.extend_from_slice(&(*count as u64).to_le_bytes());
            }
            bytes
        }
        CompressionMethod::Delta => {
            let deltas = delta_encode(values);
            let mut bytes = Vec::new();
            bytes.extend_from_slice(&(deltas.len() as u32).to_le_bytes());
            for d in &deltas {
                bytes.extend_from_slice(&d.to_le_bytes());
            }
            bytes
        }
        CompressionMethod::Quantize => {
            let levels = config.precision.unwrap_or(16) as usize;
            let indices = quantize(values, levels);
            let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
            let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let mut bytes = Vec::new();
            bytes.extend_from_slice(&(indices.len() as u32).to_le_bytes());
            bytes.extend_from_slice(&(levels as u32).to_le_bytes());
            bytes.extend_from_slice(&min.to_le_bytes());
            bytes.extend_from_slice(&max.to_le_bytes());
            bytes.extend_from_slice(&indices);
            bytes
        }
        CompressionMethod::Dictionary | CompressionMethod::Huffman => {
            // Fallback: naive byte serialization of f64 values
            let mut bytes = Vec::new();
            bytes.extend_from_slice(&(values.len() as u32).to_le_bytes());
            for v in values {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
            bytes
        }
    }
}

pub fn decompress_tiles(data: &[u8], config: &CompressionConfig) -> Vec<f64> {
    if data.is_empty() {
        return Vec::new();
    }
    match config.method {
        CompressionMethod::RunLength => {
            let n = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
            let mut values = Vec::with_capacity(n * 16); // rough estimate
            let mut offset = 4;
            for _ in 0..n {
                let val = f64::from_le_bytes(data[offset..offset + 8].try_into().unwrap());
                offset += 8;
                let count = u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap()) as usize;
                offset += 8;
                values.extend(std::iter::repeat(val).take(count));
            }
            values
        }
        CompressionMethod::Delta => {
            let n = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
            let mut deltas = Vec::with_capacity(n);
            let mut offset = 4;
            for _ in 0..n {
                let d = f64::from_le_bytes(data[offset..offset + 8].try_into().unwrap());
                deltas.push(d);
                offset += 8;
            }
            delta_decode(&deltas)
        }
        CompressionMethod::Quantize => {
            let n = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
            let levels = u32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;
            let min = f64::from_le_bytes(data[8..16].try_into().unwrap());
            let max = f64::from_le_bytes(data[16..24].try_into().unwrap());
            let indices: Vec<u8> = data[24..24 + n].to_vec();
            dequantize(&indices, levels, min, max)
        }
        CompressionMethod::Dictionary | CompressionMethod::Huffman => {
            let n = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
            let mut values = Vec::with_capacity(n);
            let mut offset = 4;
            for _ in 0..n {
                let v = f64::from_le_bytes(data[offset..offset + 8].try_into().unwrap());
                values.push(v);
                offset += 8;
            }
            values
        }
    }
}

// ── Utilities ───────────────────────────────────────────────────────────────

pub fn compression_ratio(original: &[f64], compressed: &[u8]) -> f64 {
    let original_bytes = original.len() * 8;
    if compressed.is_empty() {
        return if original_bytes == 0 { 1.0 } else { 0.0 };
    }
    original_bytes as f64 / compressed.len() as f64
}

pub fn estimate_compressibility(data: &[f64]) -> f64 {
    if data.len() < 2 {
        return 1.0;
    }
    // Use multiple heuristics and average them

    // 1. RLE potential: ratio of unique runs to total length
    let runs = run_length_encode(data).len() as f64;
    let rle_score = 1.0 - (runs - 1.0) / (data.len() - 1) as f64;

    // 2. Delta potential: small deltas → high compressibility
    let deltas = delta_encode(data);
    let max_val = data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let min_val = data.iter().cloned().fold(f64::INFINITY, f64::min);
    let range = (max_val - min_val).abs().max(1e-10);
    let avg_delta: f64 = deltas[1..].iter().map(|d| d.abs()).sum::<f64>() / (deltas.len() - 1) as f64;
    let delta_score = 1.0 - (avg_delta / range).min(1.0);

    // 3. Variance-based: low variance = high compressibility
    let mean: f64 = data.iter().sum::<f64>() / data.len() as f64;
    let variance: f64 = data.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / data.len() as f64;
    let var_score = 1.0 - (variance / (range * range)).min(1.0);

    (rle_score + delta_score + var_score) / 3.0
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── RLE tests ──

    #[test]
    fn rle_roundtrip() {
        let data = vec![1.0, 1.0, 1.0, 2.0, 2.0, 3.0, 3.0, 3.0, 3.0];
        let encoded = run_length_encode(&data);
        let decoded = run_length_decode(&encoded);
        assert_eq!(data, decoded);
    }

    #[test]
    fn rle_repeated_compresses_well() {
        let data = vec![5.0; 1000];
        let encoded = run_length_encode(&data);
        assert_eq!(encoded.len(), 1);
        assert_eq!(encoded[0], (5.0, 1000));
    }

    #[test]
    fn rle_random_doesnt_compress() {
        let data: Vec<f64> = (0..100).map(|i| i as f64 * 0.123).collect();
        let encoded = run_length_encode(&data);
        assert_eq!(encoded.len(), data.len());
    }

    // ── Delta tests ──

    #[test]
    fn delta_roundtrip() {
        let data = vec![10.0, 12.0, 15.0, 19.0, 24.0];
        let encoded = delta_encode(&data);
        let decoded = delta_decode(&encoded);
        assert_eq!(data, decoded);
    }

    #[test]
    fn delta_smooth_data_small_deltas() {
        let data: Vec<f64> = (0..100).map(|i| i as f64 * 0.1).collect();
        let deltas = delta_encode(&data);
        // All deltas should be ~0.1
        for d in &deltas[1..] {
            assert!((d - 0.1).abs() < 1e-10);
        }
    }

    #[test]
    fn delta_noisy_data_large_deltas() {
        let data = vec![0.0, 1000.0, -500.0, 900.0, -200.0];
        let deltas = delta_encode(&data);
        assert!(deltas[1].abs() > 100.0);
    }

    // ── Quantize tests ──

    #[test]
    fn quantize_roundtrip() {
        let data = vec![0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0];
        let levels = 8;
        let min = 0.0;
        let max = 3.0;
        let indices = quantize(&data, levels);
        let recovered = dequantize(&indices, levels, min, max);
        // Check acceptable loss
        for (orig, rec) in data.iter().zip(recovered.iter()) {
            assert!((orig - rec).abs() < 1.0);
        }
    }

    #[test]
    fn quantize_correct_number_of_levels() {
        let data = vec![0.0, 1.0, 2.0, 3.0, 4.0];
        let levels = 4;
        let indices = quantize(&data, levels);
        assert!(indices.iter().all(|&i| i < levels as u8));
    }

    #[test]
    fn quantize_more_levels_less_loss() {
        let data = vec![0.0, 0.5, 1.0, 1.5, 2.0];
        let min = 0.0_f64;
        let max = 2.0_f64;

        let indices_low = quantize(&data, 2);
        let recovered_low = dequantize(&indices_low, 2, min, max);

        let indices_high = quantize(&data, 64);
        let recovered_high = dequantize(&indices_high, 64, min, max);

        let err_low: f64 = data.iter().zip(recovered_low.iter()).map(|(a, b)| (a - b).abs()).sum();
        let err_high: f64 = data.iter().zip(recovered_high.iter()).map(|(a, b)| (a - b).abs()).sum();

        assert!(err_high < err_low);
    }

    // ── Dictionary tests ──

    #[test]
    fn dictionary_roundtrip() {
        let data = vec![
            "hello".to_string(),
            "world".to_string(),
            "hello".to_string(),
            "foo".to_string(),
            "world".to_string(),
        ];
        let (indices, dict) = dictionary_encode(&data);
        let decoded = dictionary_decode(&indices, &dict);
        assert_eq!(data, decoded);
    }

    // ── Compression ratio ──

    #[test]
    fn compression_ratio_calculation() {
        let original = vec![1.0; 100];
        let config = CompressionConfig {
            method: CompressionMethod::RunLength,
            lossy: false,
            precision: None,
        };
        let compressed = compress_tiles(&original, &config);
        let ratio = compression_ratio(&original, &compressed);
        // 100 f64 = 800 bytes, compressed is much smaller
        assert!(ratio > 5.0);
    }

    // ── Compressibility ──

    #[test]
    fn compressibility_constant_is_one() {
        let data = vec![42.0; 50];
        let score = estimate_compressibility(&data);
        assert!(score > 0.95);
    }

    #[test]
    fn compressibility_random_near_zero() {
        let data: Vec<f64> = (0..100).map(|i| (i as f64 * 7919.0).sin()).collect();
        let score = estimate_compressibility(&data);
        assert!(score < 0.7);
    }

    // ── Edge cases ──

    #[test]
    fn empty_data() {
        let data: Vec<f64> = vec![];
        assert!(run_length_encode(&data).is_empty());
        assert!(delta_encode(&data).is_empty());
        assert!(quantize(&data, 8).is_empty());

        let config = CompressionConfig {
            method: CompressionMethod::RunLength,
            lossy: false,
            precision: None,
        };
        let compressed = compress_tiles(&data, &config);
        assert!(compressed.is_empty());
        let decompressed = decompress_tiles(&compressed, &config);
        assert!(decompressed.is_empty());
    }

    #[test]
    fn single_value() {
        let data = vec![7.0];
        assert_eq!(run_length_encode(&data), vec![(7.0, 1)]);
        assert_eq!(delta_encode(&data), vec![7.0]);
        assert_eq!(run_length_decode(&run_length_encode(&data)), data);
    }

    #[test]
    fn all_same() {
        let data = vec![3.14; 500];
        let encoded = run_length_encode(&data);
        assert_eq!(encoded.len(), 1);
        let decoded = run_length_decode(&encoded);
        assert_eq!(data, decoded);
    }

    #[test]
    fn all_different() {
        let data: Vec<f64> = (0..50).map(|i| i as f64).collect();
        let encoded = run_length_encode(&data);
        assert_eq!(encoded.len(), data.len());
    }

    // ── Compare methods ──

    #[test]
    fn compare_methods_same_data() {
        let data: Vec<f64> = (0..200).map(|i| (i as f64 * 0.05).sin()).collect();

        let configs = [
            CompressionConfig { method: CompressionMethod::RunLength, lossy: false, precision: None },
            CompressionConfig { method: CompressionMethod::Delta, lossy: false, precision: None },
            CompressionConfig { method: CompressionMethod::Quantize, lossy: true, precision: Some(32) },
        ];

        let mut sizes: Vec<usize> = Vec::new();
        for config in &configs {
            let compressed = compress_tiles(&data, config);
            let decompressed = decompress_tiles(&compressed, config);
            sizes.push(compressed.len());
            if !config.lossy {
                for (a, b) in data.iter().zip(decompressed.iter()) {
                    assert!((a - b).abs() < 1e-10, "mismatch: {} vs {}", a, b);
                }
            }
        }
        // Just verify they all produced output
        assert!(sizes.iter().all(|&s| s > 0));
    }

    #[test]
    fn compress_decompress_roundtrip_rle() {
        let data = vec![1.0, 1.0, 2.0, 2.0, 2.0, 3.0];
        let config = CompressionConfig {
            method: CompressionMethod::RunLength,
            lossy: false,
            precision: None,
        };
        let compressed = compress_tiles(&data, &config);
        let decompressed = decompress_tiles(&compressed, &config);
        assert_eq!(data, decompressed);
    }

    #[test]
    fn compress_decompress_roundtrip_delta() {
        let data = vec![10.0, 20.0, 30.0, 40.0, 50.0];
        let config = CompressionConfig {
            method: CompressionMethod::Delta,
            lossy: false,
            precision: None,
        };
        let compressed = compress_tiles(&data, &config);
        let decompressed = decompress_tiles(&compressed, &config);
        assert_eq!(data, decompressed);
    }
}
