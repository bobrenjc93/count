use count::{CountDB, CountConfig, DataPoint, SeriesKey};
use count::query::AggregationType;
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Count Database - Basic Usage Example");
    println!("===================================");
    
    // Create database with custom configuration
    let mut config = CountConfig::default();
    config.memory_buffer_size = 5000;    // Keep 5000 points in memory per series
    config.flush_interval_seconds = 300; // Flush to disk every 5 minutes
    config.data_dir = "./example_data".to_string();
    
    let db = CountDB::new(config).await?;
    
    // Example 1: Insert CPU metrics
    println!("\n1. Inserting CPU metrics...");
    let cpu_series = SeriesKey::from("server1.cpu.usage");
    let start_time = get_current_timestamp();
    
    // Simulate CPU usage data over 5 minutes
    for i in 0..300 {
        let timestamp = start_time + i * 1000; // 1-second intervals
        let cpu_usage = 20.0 + (i as f64 / 10.0).sin() * 15.0 + rand::random::<f64>() * 10.0;
        let point = DataPoint::new(timestamp, cpu_usage.max(0.0).min(100.0));
        
        db.insert(cpu_series.clone(), point).await?;
    }
    
    println!("   ✓ Inserted 300 CPU usage points");
    
    // Example 2: Insert Memory metrics
    println!("\n2. Inserting Memory metrics...");
    let memory_series = SeriesKey::from("server1.memory.usage");
    
    for i in 0..300 {
        let timestamp = start_time + i * 1000;
        let memory_usage = 4096.0 + (i as f64 / 20.0).cos() * 1024.0 + rand::random::<f64>() * 512.0;
        let point = DataPoint::new(timestamp, memory_usage.max(0.0));
        
        db.insert(memory_series.clone(), point).await?;
    }
    
    println!("   ✓ Inserted 300 Memory usage points");
    
    // Example 3: Query recent data
    println!("\n3. Querying recent data...");
    let last_minute = start_time + 240 * 1000; // Last minute of data
    let end_time = start_time + 300 * 1000;
    
    let recent_cpu = db.query_range(cpu_series.clone(), last_minute, end_time).await?;
    println!("   ✓ Found {} CPU points in last minute", recent_cpu.len());
    
    if !recent_cpu.is_empty() {
        println!("   📊 Sample recent CPU values:");
        for (i, point) in recent_cpu.iter().take(5).enumerate() {
            println!("      {}. {} -> {:.2}%", i + 1, 
                    format_timestamp(point.timestamp), point.value);
        }
    }
    
    // Example 4: Aggregation queries
    println!("\n4. Running aggregation queries...");
    
    let aggregations = vec![
        (AggregationType::Mean, "Average CPU"),
        (AggregationType::Max, "Peak CPU"),
        (AggregationType::Min, "Minimum CPU"),
        (AggregationType::Count, "CPU Sample Count"),
    ];
    
    for (agg_type, description) in aggregations {
        let result = db.query_aggregated(
            cpu_series.clone(), 
            start_time, 
            end_time, 
            agg_type
        ).await?;
        
        match agg_type {
            AggregationType::Count => println!("   📈 {}: {:.0}", description, result),
            _ => println!("   📈 {}: {:.2}%", description, result),
        }
    }
    
    // Example 5: Cross-series analysis
    println!("\n5. Cross-series analysis...");
    
    let cpu_avg = db.query_aggregated(
        cpu_series.clone(), 
        start_time, 
        end_time, 
        AggregationType::Mean
    ).await?;
    
    let memory_avg = db.query_aggregated(
        memory_series.clone(), 
        start_time, 
        end_time, 
        AggregationType::Mean
    ).await?;
    
    println!("   🖥️  Average CPU Usage: {:.2}%", cpu_avg);
    println!("   💾 Average Memory Usage: {:.2} MB", memory_avg);
    
    if cpu_avg > 50.0 {
        println!("   ⚠️  High CPU usage detected!");
    }
    
    if memory_avg > 6000.0 {
        println!("   ⚠️  High memory usage detected!");
    }
    
    // Example 6: Time-based filtering
    println!("\n6. Time-based filtering...");
    
    let time_windows = vec![
        (start_time, start_time + 60 * 1000, "First minute"),
        (start_time + 120 * 1000, start_time + 180 * 1000, "Minutes 2-3"),
        (start_time + 240 * 1000, end_time, "Last minute"),
    ];
    
    for (window_start, window_end, description) in time_windows {
        let points = db.query_range(cpu_series.clone(), window_start, window_end).await?;
        let avg = if !points.is_empty() {
            points.iter().map(|p| p.value).sum::<f64>() / points.len() as f64
        } else {
            0.0
        };
        
        println!("   ⏰ {}: {} points, avg {:.2}%", description, points.len(), avg);
    }
    
    // Example 7: Performance demonstration
    println!("\n7. Performance test...");
    
    let perf_series = SeriesKey::from("performance.test");
    let perf_start = std::time::Instant::now();
    
    // Insert 1000 points rapidly
    for i in 0..1000 {
        let point = DataPoint::new(start_time + 300000 + i * 100, i as f64);
        db.insert(perf_series.clone(), point).await?;
    }
    
    let insert_duration = perf_start.elapsed();
    let throughput = 1000.0 / insert_duration.as_secs_f64();
    
    println!("   ⚡ Inserted 1000 points in {:?}", insert_duration);
    println!("   📊 Throughput: {:.0} points/second", throughput);
    
    // Query performance
    let query_start = std::time::Instant::now();
    let all_points = db.query_range(perf_series, start_time + 300000, start_time + 400000).await?;
    let query_duration = query_start.elapsed();
    
    println!("   🔍 Queried {} points in {:?}", all_points.len(), query_duration);
    
    // Cleanup
    println!("\n8. Shutting down...");
    db.shutdown().await?;
    println!("   ✓ Database shutdown complete");
    
    println!("\n🎉 Example completed successfully!");
    println!("    Data has been persisted to './example_data'");
    println!("    You can restart this example to see data persistence in action.");
    
    Ok(())
}

fn get_current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn format_timestamp(timestamp: u64) -> String {
    let duration = std::time::Duration::from_millis(timestamp);
    let datetime = std::time::UNIX_EPOCH + duration;
    
    // Simple timestamp formatting (you might want to use chrono in production)
    format!("{:?}", datetime)
        .split_once('.')
        .map(|(s, _)| s)
        .unwrap_or("invalid")
        .replace("SystemTime { tv_sec: ", "")
        .replace(", tv_nsec: 0 }", "")
}