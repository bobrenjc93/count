use criterion::{black_box, criterion_group, criterion_main, Criterion};
use count::compression::{TimestampCompressor, ValueCompressor};

fn benchmark_timestamp_compression(c: &mut Criterion) {
    c.bench_function("timestamp_compression_regular", |b| {
        b.iter(|| {
            let mut compressor = TimestampCompressor::new();
            // Regular 60-second intervals
            let timestamps = (0..1000).map(|i| 1000 + i * 60_000).collect::<Vec<_>>();
            
            for &ts in &timestamps {
                black_box(compressor.compress(ts).unwrap());
            }
        })
    });
    
    c.bench_function("timestamp_compression_irregular", |b| {
        b.iter(|| {
            let mut compressor = TimestampCompressor::new();
            // Irregular intervals with some randomness
            let mut timestamps = Vec::new();
            let mut current = 1000u64;
            for _ in 0..1000 {
                current += 50_000 + (current % 20_000); // Irregular intervals
                timestamps.push(current);
            }
            
            for &ts in &timestamps {
                black_box(compressor.compress(ts).unwrap());
            }
        })
    });
}

fn benchmark_value_compression(c: &mut Criterion) {
    c.bench_function("value_compression_identical", |b| {
        b.iter(|| {
            let mut compressor = ValueCompressor::new();
            // Identical values - should compress very well
            let value = 42.5f64;
            
            for _ in 0..1000 {
                black_box(compressor.compress(value).unwrap());
            }
        })
    });
    
    c.bench_function("value_compression_similar", |b| {
        b.iter(|| {
            let mut compressor = ValueCompressor::new();
            // Similar values with small variations
            let base_value = 100.0f64;
            
            for i in 0..1000 {
                let value = base_value + (i as f64 * 0.1);
                black_box(compressor.compress(value).unwrap());
            }
        })
    });
    
    c.bench_function("value_compression_random", |b| {
        b.iter(|| {
            let mut compressor = ValueCompressor::new();
            // Random values - worst case for compression
            
            for i in 0..1000 {
                let value = (i as f64 * 3.14159).sin() * 1000.0 + (i as f64).cos() * 500.0;
                black_box(compressor.compress(value).unwrap());
            }
        })
    });
}

fn benchmark_compression_ratio(c: &mut Criterion) {
    c.bench_function("compression_ratio_measurement", |b| {
        b.iter(|| {
            let mut ts_compressor = TimestampCompressor::new();
            let mut val_compressor = ValueCompressor::new();
            
            let mut total_original_size = 0usize;
            let mut total_compressed_size = 0usize;
            
            // Generate realistic time series data
            let base_time = 1640995200000u64; // 2022-01-01
            for i in 0..1000 {
                let timestamp = base_time + i * 60_000; // 1-minute intervals
                let value = 50.0 + (i as f64 / 10.0).sin() * 20.0; // Realistic CPU usage pattern
                
                let compressed_ts = ts_compressor.compress(timestamp).unwrap();
                let compressed_val = val_compressor.compress(value).unwrap();
                
                total_original_size += 8 + 8; // u64 + f64
                total_compressed_size += compressed_ts.len() + compressed_val.len();
            }
            
            let compression_ratio = total_original_size as f64 / total_compressed_size as f64;
            black_box(compression_ratio);
        })
    });
}

criterion_group!(
    benches,
    benchmark_timestamp_compression,
    benchmark_value_compression,
    benchmark_compression_ratio
);
criterion_main!(benches);