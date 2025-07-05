use count::{CountDB, CountConfig, DataPoint, SeriesKey};
use count::query::AggregationType;
use rand::Rng;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Count - High-Performance Time Series Database");
    println!("===========================================");
    
    let config = CountConfig::default();
    let mut db = CountDB::new(config).await?;
    
    println!("Generating fake CPU metrics...");
    generate_fake_cpu_data(&mut db).await?;
    
    println!("\nRunning queries and aggregations...");
    run_queries(&db).await?;
    
    println!("\nShutting down database...");
    db.shutdown().await?;
    
    println!("Demo completed successfully!");
    Ok(())
}

async fn generate_fake_cpu_data(db: &mut CountDB) -> Result<(), Box<dyn std::error::Error>> {
    let mut rng = rand::thread_rng();
    let start_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis() as u64 - 3600 * 1000; // Start 1 hour ago
    
    let series_names = vec![
        "cpu.usage.total",
        "cpu.usage.user", 
        "cpu.usage.system",
        "cpu.usage.idle",
        "memory.usage.percent",
        "memory.usage.bytes",
        "disk.io.read_bytes",
        "disk.io.write_bytes",
    ];
    
    println!("Inserting {} series with 3600 points each...", series_names.len());
    
    for series_name in &series_names {
        let series = SeriesKey::from(*series_name);
        let mut base_value = match series_name {
            s if s.contains("cpu.usage") => rng.gen_range(10.0..80.0),
            s if s.contains("memory") => rng.gen_range(1000.0..8000.0),
            s if s.contains("disk") => rng.gen_range(1000000.0..10000000.0),
            _ => rng.gen_range(0.0..100.0),
        };
        
        // Generate 3600 points (1 hour of data at 1-second intervals)
        for i in 0..3600 {
            let timestamp = start_time + i * 1000; // 1-second intervals
            
            // Add some realistic variation
            let variation = rng.gen_range(-5.0..5.0);
            base_value = f64::max(base_value + variation, 0.0);
            
            // Add some periodic patterns
            let time_factor = (i as f64 / 3600.0) * 2.0 * std::f64::consts::PI;
            let periodic_factor = (time_factor.sin() * 10.0) + (time_factor.cos() * 5.0);
            let final_value = base_value + periodic_factor;
            
            let point = DataPoint::new(timestamp, final_value);
            db.insert(series.clone(), point).await?;
            
            // Small delay to make it feel more realistic
            if i % 100 == 0 {
                sleep(Duration::from_millis(1)).await;
                print!(".");
                if i % 1000 == 0 {
                    println!(" {} points inserted for {}", i, series_name);
                }
            }
        }
        println!(" ✓ Completed {}", series_name);
    }
    
    Ok(())
}

async fn run_queries(db: &CountDB) -> Result<(), Box<dyn std::error::Error>> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis() as u64;
    
    let one_hour_ago = now - 3600 * 1000;
    let thirty_min_ago = now - 1800 * 1000;
    let ten_min_ago = now - 600 * 1000;
    
    println!("\n📊 Query Results:");
    println!("================");
    
    // Test different time ranges and aggregations
    let test_cases = vec![
        ("cpu.usage.total", one_hour_ago, now, AggregationType::Mean, "Last Hour Average"),
        ("cpu.usage.total", thirty_min_ago, now, AggregationType::Max, "Last 30min Peak"),
        ("memory.usage.percent", ten_min_ago, now, AggregationType::Min, "Last 10min Minimum"),
        ("disk.io.read_bytes", one_hour_ago, now, AggregationType::Sum, "Last Hour Total Reads"),
        ("cpu.usage.system", one_hour_ago, now, AggregationType::Count, "Total System CPU Points"),
    ];
    
    for (series_name, start, end, agg_type, description) in test_cases {
        let series = SeriesKey::from(series_name);
        
        // Test range query
        let range_start_time = std::time::Instant::now();
        let points = db.query_range(series.clone(), start, end).await?;
        let range_duration = range_start_time.elapsed();
        
        // Test aggregation query
        let agg_start_time = std::time::Instant::now();
        let agg_result = db.query_aggregated(series, start, end, agg_type).await?;
        let agg_duration = agg_start_time.elapsed();
        
        println!("🔍 {}", description);
        println!("   Series: {}", series_name);
        println!("   Time Range: {} to {}", format_timestamp(start), format_timestamp(end));
        println!("   Points Found: {}", points.len());
        println!("   {:?} Result: {:.2}", agg_type, agg_result);
        println!("   Range Query Time: {:?}", range_duration);
        println!("   Aggregation Time: {:?}", agg_duration);
        
        // Show sample of data points
        if !points.is_empty() {
            println!("   Sample Points:");
            let sample_size = std::cmp::min(5, points.len());
            for i in 0..sample_size {
                let point = &points[i];
                println!("     {} -> {:.2}", format_timestamp(point.timestamp), point.value);
            }
            if points.len() > sample_size {
                println!("     ... and {} more points", points.len() - sample_size);
            }
        }
        println!();
    }
    
    // Test query correctness with known data
    println!("🧪 Verification Tests:");
    println!("=====================");
    
    // Insert known test data
    let test_series = SeriesKey::from("test.verification");
    let test_values = vec![10.0, 20.0, 30.0, 40.0, 50.0];
    let base_time = now;
    
    for (i, &value) in test_values.iter().enumerate() {
        let point = DataPoint::new(base_time + i as u64 * 1000, value);
        db.insert(test_series.clone(), point).await?;
    }
    
    // Verify queries
    let verification_tests = vec![
        (AggregationType::Sum, 150.0, "Sum should be 150.0"),
        (AggregationType::Mean, 30.0, "Mean should be 30.0"),
        (AggregationType::Min, 10.0, "Min should be 10.0"),
        (AggregationType::Max, 50.0, "Max should be 50.0"),
        (AggregationType::Count, 5.0, "Count should be 5.0"),
    ];
    
    for (agg_type, expected, description) in verification_tests {
        let result = db.query_aggregated(test_series.clone(), base_time, base_time + 10000, agg_type).await?;
        let passed = (result - expected).abs() < f64::EPSILON;
        
        println!("✅ {}: {:.1} (expected {:.1}) - {}", 
                 description, 
                 result, 
                 expected,
                 if passed { "PASS" } else { "FAIL" });
    }
    
    Ok(())
}

fn format_timestamp(timestamp: u64) -> String {
    let duration = std::time::Duration::from_millis(timestamp);
    let datetime = std::time::UNIX_EPOCH + duration;
    format!("{:?}", datetime).split_once('.').unwrap_or(("", "")).0.to_string()
}