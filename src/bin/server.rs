use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{interval, Duration};
use tracing::{info, error};
use storage::TSMap;
use query::{QueryEngine, Query, QueryResult, Aggregation};
use tsdb_core::DataPoint;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    
    info!("Starting Gorilla TSDB Server");
    
    let storage = Arc::new(TSMap::new());
    let query_engine = Arc::new(QueryEngine::new(storage.clone()));
    
    let storage_for_cleanup = storage.clone();
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            storage_for_cleanup.seal_expired_blocks();
        }
    });
    
    simulate_data_ingestion(storage.clone()).await;
    
    demo_queries(query_engine).await;
    
    Ok(())
}

async fn simulate_data_ingestion(storage: Arc<TSMap>) {
    info!("Starting data ingestion simulation");
    
    let keys = vec![
        "cpu.usage".to_string(),
        "memory.usage".to_string(),
        "disk.io".to_string(),
        "network.bytes".to_string(),
    ];
    
    let start_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    
    for i in 0..1000 {
        let timestamp = start_time + i * 1000; // Every second
        
        for (idx, key) in keys.iter().enumerate() {
            let value = simulate_metric_value(idx, i);
            let point = DataPoint::new(timestamp, value);
            
            if let Err(e) = storage.insert(key.clone(), point) {
                error!("Failed to insert point for {}: {}", key, e);
            }
        }
        
        if i % 100 == 0 {
            info!("Ingested {} batches of data", i);
        }
    }
    
    let stats = storage.get_stats();
    info!("Data ingestion complete. Stats: {:?}", stats);
}

fn simulate_metric_value(metric_idx: usize, time_idx: u64) -> f64 {
    let base = (time_idx as f64 * 0.1).sin();
    
    match metric_idx {
        0 => 50.0 + base * 20.0 + (time_idx % 10) as f64, // CPU usage
        1 => 60.0 + base * 15.0 + (time_idx % 5) as f64 * 2.0, // Memory usage
        2 => 1000.0 + base * 500.0 + (time_idx % 20) as f64 * 10.0, // Disk IO
        3 => 10000.0 + base * 2000.0 + (time_idx % 15) as f64 * 100.0, // Network bytes
        _ => 0.0,
    }
}

async fn demo_queries(query_engine: Arc<QueryEngine>) {
    info!("Running demo queries");
    
    let keys = query_engine.list_keys();
    info!("Available metrics: {:?}", keys);
    
    if let Some(key) = keys.first() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        let start_time = now - 300_000; // 5 minutes ago
        let end_time = now;
        
        info!("Querying {} from {} to {}", key, start_time, end_time);
        
        let query = Query::new(key.clone(), start_time, end_time);
        match query_engine.execute(query) {
            Ok(QueryResult::Points(points)) => {
                info!("Retrieved {} raw points", points.len());
                if !points.is_empty() {
                    info!("First point: {:?}", points[0]);
                    info!("Last point: {:?}", points.last().unwrap());
                }
            },
            Ok(QueryResult::Aggregated(_)) => {
                info!("Unexpected aggregated result");
            },
            Err(e) => {
                error!("Query failed: {}", e);
            }
        }
        
        let agg_query = Query::new(key.clone(), start_time, end_time)
            .with_aggregation(Aggregation::Avg, 60_000); // 1-minute windows
        
        match query_engine.execute(agg_query) {
            Ok(QueryResult::Aggregated(points)) => {
                info!("Retrieved {} aggregated points", points.len());
                for point in &points {
                    info!("Aggregated: ts={}, value={:.2}, count={}", 
                          point.timestamp, point.value, point.count);
                }
            },
            Ok(QueryResult::Points(_)) => {
                info!("Unexpected raw points result");
            },
            Err(e) => {
                error!("Aggregation query failed: {}", e);
            }
        }
    }
    
    let stats = query_engine.get_storage_stats();
    info!("Final storage stats: {:?}", stats);
}