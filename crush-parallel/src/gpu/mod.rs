//! GPU-accelerated compression (feature: `gpu`).
//!
//! This entire module is compiled only when the `gpu` feature is enabled.
//! Default builds have zero GPU symbols.

pub mod worker;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use crate::config::EngineConfiguration;
    use crate::engine::{compress, decompress};

    #[test]
    fn test_gpu_produces_identical_output_to_cpu() {
        // Skip when no GPU adapter is available.
        if super::worker::GpuWorker::new().is_none() {
            return;
        }
        let data: Vec<u8> = b"gpu test".iter().cycle().take(200_000).copied().collect();
        let cpu_config = EngineConfiguration::builder()
            .block_size(65_536)
            .gpu(false)
            .build()
            .expect("config");
        let gpu_config = EngineConfiguration::builder()
            .block_size(65_536)
            .gpu(true)
            .build()
            .expect("config");
        let cpu_compressed = compress(&data, &cpu_config).expect("cpu compress");
        let gpu_compressed = compress(&data, &gpu_config).expect("gpu compress");
        // Both must decompress to identical data
        let cpu_recovered = decompress(&cpu_compressed, &cpu_config).expect("cpu decompress");
        let gpu_recovered = decompress(&gpu_compressed, &cpu_config).expect("gpu decompress");
        assert_eq!(cpu_recovered, gpu_recovered);
    }

    #[test]
    fn test_gpu_fallback_when_no_adapter() {
        // Verify that even when GPU is requested, compression completes via CPU fallback.
        let data: Vec<u8> = b"fallback test"
            .iter()
            .cycle()
            .take(100_000)
            .copied()
            .collect();
        let config = EngineConfiguration::builder()
            .block_size(65_536)
            .gpu(true)
            .build()
            .expect("config");
        // Must not error even without a GPU adapter
        let result = compress(&data, &config);
        assert!(result.is_ok(), "GPU fallback failed: {result:?}");
    }
}
