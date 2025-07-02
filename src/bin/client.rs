use std::time::{SystemTime, UNIX_EPOCH};
use tsdb_core::DataPoint;
use storage::TSMap;
use query::{QueryEngine, Query, QueryResult, Aggregation};
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Gorilla TSDB Client Demo");
    
    let storage = Arc::new(TSMap::new());
    let query_engine = QueryEngine::new(storage.clone());
    
    // Load some sample data
    load_sample_data(&storage)?;
    
    // Demo basic queries
    demo_basic_queries(&query_engine)?;
    
    // Demo aggregation queries
    demo_aggregation_queries(&query_engine)?;
    
    // Demo compression efficiency
    demo_compression_stats(&storage);
    
    Ok(())
}

fn load_sample_data(storage: &TSMap) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Loading Sample Data ===");
    
    let base_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis() as u64 - 3600_000; // 1 hour ago
    
    let metrics = vec![
        ("server.cpu.percent", generate_cpu_data(base_time)),
        ("server.memory.bytes", generate_memory_data(base_time)),
        ("server.network.rx_bytes", generate_network_data(base_time)),
    ];
    
    for (key, points) in metrics {
        println!("Loading {} points for {}", points.len(), key);
        for point in points {
            storage.insert(key.to_string(), point)?;
        }
    }
    
    let stats = storage.get_stats();
    println!("Loaded data: {} series, {} total points", 
             stats.series_count, stats.total_points);
    
    Ok(())
}

fn generate_cpu_data(base_time: u64) -> Vec<DataPoint> {
    let mut points = Vec::new();
    
    for i in 0..3600 { // 1 hour of data, 1 second intervals
        let timestamp = base_time + i * 1000;
        
        // Simulate CPU usage: baseline + sine wave + some randomness
        let baseline = 30.0;
        let wave = 20.0 * (i as f64 * 0.01).sin();
        let noise = (i % 17) as f64 * 0.5;
        let value = baseline + wave + noise;
        
        points.push(DataPoint::new(timestamp, value));
    }
    
    points
}

fn generate_memory_data(base_time: u64) -> Vec<DataPoint> {
    let mut points = Vec::new();
    let mut current_mem = 4_000_000_000.0; // 4GB baseline
    
    for i in 0..3600 {
        let timestamp = base_time + i * 1000;
        
        // Simulate memory usage with gradual increase and some spikes
        let growth = i as f64 * 1000.0;
        let spike = if i % 300 == 0 { 500_000_000.0 } else { 0.0 };
        
        current_mem = 4_000_000_000.0 + growth + spike;
        points.push(DataPoint::new(timestamp, current_mem));
    }
    
    points
}

fn generate_network_data(base_time: u64) -> Vec<DataPoint> {
    let mut points = Vec::new();
    
    for i in 0..3600 {
        let timestamp = base_time + i * 1000;
        
        // Simulate network data with bursts
        let base_rate = 1_000_000.0; // 1 MB/s baseline
        let burst = if i % 60 < 10 { 10_000_000.0 } else { 0.0 }; // 10MB/s bursts
        let variation = (i as f64 * 0.05).cos() * 500_000.0;
        
        let value = base_rate + burst + variation;
        points.push(DataPoint::new(timestamp, value));
    }
    
    points
}

fn demo_basic_queries(query_engine: &QueryEngine) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Basic Query Demo ===");
    
    let keys = query_engine.list_keys();
    println!("Available metrics: {:?}", keys);
    
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis() as u64;
    
    for key in &keys {
        let query = Query::new(
            key.clone(),
            now - 600_000, // Last 10 minutes
            now
        ).with_max_points(20); // Downsample to 20 points
        
        match query_engine.execute(query)? {
            QueryResult::Points(points) => {
                println!("\n{}: {} points", key, points.len());
                if !points.is_empty() {
                    println!("  First: {:?}", points.first().unwrap());
                    println!("  Last:  {:?}", points.last().unwrap());
                }
            },
            _ => println!("Unexpected result type"),
        }
    }
    
    Ok(())
}

fn demo_aggregation_queries(query_engine: &QueryEngine) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Aggregation Query Demo ===");
    
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis() as u64;
    
    let aggregations = vec![
        ("Average", Aggregation::Avg),
        ("Maximum", Aggregation::Max),
        ("Minimum", Aggregation::Min),
        ("Sum", Aggregation::Sum),
    ];
    
    let cpu_key = "server.cpu.percent".to_string();
    
    for (name, agg) in aggregations {
        let query = Query::new(
            cpu_key.clone(),
            now - 1800_000, // Last 30 minutes
            now
        ).with_aggregation(agg, 300_000); // 5-minute windows
        
        match query_engine.execute(query)? {
            QueryResult::Aggregated(points) => {
                println!("\n{} CPU usage (5-min windows): {} points", name, points.len());
                for (i, point) in points.iter().take(3).enumerate() {
                    println!("  Window {}: {:.2}% ({} samples)", 
                             i + 1, point.value, point.count);
                }
            },
            _ => println!("Unexpected result type"),
        }
    }
    
    Ok(())
}

fn demo_compression_stats(storage: &TSMap) {
    println!("\n=== Compression Statistics ===");
    
    let stats = storage.get_stats();
    println!("Total series: {}", stats.series_count);
    println!("Total data points: {}", stats.total_points);
    println!("Total compressed blocks: {}", stats.total_blocks);
    println!("Total compressed size: {} bytes", stats.total_compressed_size);
    
    // Estimate uncompressed size (rough calculation)
    let uncompressed_estimate = stats.total_points * (8 + 8); // 8 bytes timestamp + 8 bytes value
    let compression_ratio = if stats.total_compressed_size > 0 {
        uncompressed_estimate as f64 / stats.total_compressed_size as f64
    } else {
        0.0
    };
    
    println!("Estimated uncompressed size: {} bytes", uncompressed_estimate);
    println!("Compression ratio: {:.2}x", compression_ratio);
    
    if compression_ratio > 1.0 {
        let savings = 100.0 * (1.0 - (stats.total_compressed_size as f64 / uncompressed_estimate as f64));
        println!("Space savings: {:.1}%", savings);
    }
}