use criterion::{black_box, criterion_group, criterion_main, Criterion};
use count::{CountDB, CountConfig, DataPoint, SeriesKey};
use count::query::AggregationType;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::TempDir;
use tokio::runtime::Runtime;

fn benchmark_insert_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("insert_1000_points", |b| {
        b.iter(|| {
            rt.block_on(async {
                let temp_dir = TempDir::new().unwrap();
                let config = CountConfig {
                    memory_buffer_size: 10000,
                    flush_interval_seconds: 3600,
                    data_dir: temp_dir.path().to_string_lossy().to_string(),
                };
                
                let mut db = CountDB::new(config).await.unwrap();
                let series = SeriesKey::from("benchmark.insert");
                let base_time = get_current_timestamp();
                
                for i in 0..1000 {
                    let point = DataPoint::new(base_time + i * 1000, i as f64);
                    black_box(db.insert(series.clone(), point).await.unwrap());
                }
                
                db.shutdown().await.unwrap();
            })
        });
    });
}

fn benchmark_query_performance(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    // Setup database with test data
    let (temp_dir, config, series, base_time) = rt.block_on(async {
        let temp_dir = TempDir::new().unwrap();
        let config = CountConfig {
            memory_buffer_size: 50000,
            flush_interval_seconds: 3600,
            data_dir: temp_dir.path().to_string_lossy().to_string(),
        };
        
        let mut db = CountDB::new(config.clone()).await.unwrap();
        let series = SeriesKey::from("benchmark.query");
        let base_time = get_current_timestamp();
        
        // Insert 10,000 points for querying
        for i in 0..10000 {
            let value = 50.0 + (i as f64 / 100.0).sin() * 25.0;
            let point = DataPoint::new(base_time + i * 1000, value);
            db.insert(series.clone(), point).await.unwrap();
        }
        
        db.shutdown().await.unwrap();
        (temp_dir, config, series, base_time)
    });
    
    c.bench_function("query_range_1000_points", |b| {
        b.iter(|| {
            rt.block_on(async {
                let db = CountDB::new(config.clone()).await.unwrap();
                
                let results = db.query_range(
                    series.clone(),
                    base_time,
                    base_time + 1000 * 1000 // First 1000 points
                ).await.unwrap();
                
                black_box(results);
                db.shutdown().await.unwrap();
            })
        });
    });
    
    c.bench_function("query_range_10000_points", |b| {
        b.iter(|| {
            rt.block_on(async {
                let db = CountDB::new(config.clone()).await.unwrap();
                
                let results = db.query_range(
                    series.clone(),
                    base_time,
                    base_time + 10000 * 1000 // All 10000 points
                ).await.unwrap();
                
                black_box(results);
                db.shutdown().await.unwrap();
            })
        });
    });
    
    // Prevent temp_dir from being dropped
    std::mem::forget(temp_dir);
}

fn benchmark_aggregation_performance(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    // Setup database with test data
    let (temp_dir, config, series, base_time) = rt.block_on(async {
        let temp_dir = TempDir::new().unwrap();
        let config = CountConfig {
            memory_buffer_size: 50000,
            flush_interval_seconds: 3600,
            data_dir: temp_dir.path().to_string_lossy().to_string(),
        };
        
        let mut db = CountDB::new(config.clone()).await.unwrap();
        let series = SeriesKey::from("benchmark.aggregation");
        let base_time = get_current_timestamp();
        
        // Insert 10,000 points for aggregation
        for i in 0..10000 {
            let value = 50.0 + (i as f64 / 100.0).sin() * 25.0;
            let point = DataPoint::new(base_time + i * 1000, value);
            db.insert(series.clone(), point).await.unwrap();
        }
        
        db.shutdown().await.unwrap();
        (temp_dir, config, series, base_time)
    });
    
    let aggregation_types = vec![
        (AggregationType::Mean, "mean"),
        (AggregationType::Sum, "sum"),
        (AggregationType::Min, "min"),
        (AggregationType::Max, "max"),
        (AggregationType::Count, "count"),
    ];
    
    for (agg_type, name) in aggregation_types {
        c.bench_function(&format!("aggregation_{}_10000_points", name), |b| {
            b.iter(|| {
                rt.block_on(async {
                    let db = CountDB::new(config.clone()).await.unwrap();
                    
                    let result = db.query_aggregated(
                        series.clone(),
                        base_time,
                        base_time + 10000 * 1000,
                        agg_type
                    ).await.unwrap();
                    
                    black_box(result);
                    db.shutdown().await.unwrap();
                })
            });
        });
    }
    
    // Prevent temp_dir from being dropped
    std::mem::forget(temp_dir);
}

fn benchmark_multi_series_performance(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("multi_series_insert", |b| {
        b.iter(|| {
            rt.block_on(async {
                let temp_dir = TempDir::new().unwrap();
                let config = CountConfig {
                    memory_buffer_size: 50000,
                    flush_interval_seconds: 3600,
                    data_dir: temp_dir.path().to_string_lossy().to_string(),
                };
                
                let mut db = CountDB::new(config).await.unwrap();
                let base_time = get_current_timestamp();
                
                let series_names = vec![
                    "cpu.usage", "memory.usage", "disk.read", "disk.write", "network.in"
                ];
                
                // Insert data into 5 series simultaneously
                for i in 0..1000 {
                    for (series_idx, &series_name) in series_names.iter().enumerate() {
                        let series = SeriesKey::from(series_name);
                        let value = (series_idx as f64 + 1.0) * 10.0 + i as f64;
                        let point = DataPoint::new(base_time + i * 1000, value);
                        black_box(db.insert(series, point).await.unwrap());
                    }
                }
                
                db.shutdown().await.unwrap();
            })
        });
    });
}

fn get_current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

criterion_group!(
    benches,
    benchmark_insert_throughput,
    benchmark_query_performance,
    benchmark_aggregation_performance,
    benchmark_multi_series_performance
);
criterion_main!(benches);