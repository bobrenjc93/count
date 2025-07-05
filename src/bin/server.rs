use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use count::{
    cluster::discovery::GossipMessage,
    cluster::replication::ReplicationRequest,
    distributed::{
        coordinator::{ClusterConfig, ClusterCoordinator},
        query_router::QueryRequest,
    },
    query::AggregationType,
    CountConfig, DataPoint, SeriesKey,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use tracing::{error, info};

type AppState = Arc<RwLock<ClusterCoordinator>>;

#[derive(Debug, Deserialize)]
struct InsertRequest {
    series: String,
    timestamp: u64,
    value: f64,
}

#[derive(Debug, Deserialize)]
struct RangeQueryParams {
    start: u64,
    end: u64,
}

#[derive(Debug, Deserialize)]
struct AggregateQueryParams {
    start: u64,
    end: u64,
    aggregation: String,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    node_id: u32,
    cluster_size: usize,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    info!("Starting Count distributed server");

    // Load configuration from environment
    let count_config = CountConfig::from_env();
    let cluster_config = build_cluster_config(&count_config)?;

    info!("Node ID: {}", cluster_config.node_id);
    info!("Bind address: {}", cluster_config.bind_address);
    info!("Seed nodes: {:?}", cluster_config.seed_nodes);
    info!("Replication factor: {}", cluster_config.replication_factor);

    // Initialize cluster coordinator
    let coordinator = ClusterCoordinator::new(count_config, cluster_config.clone()).await?;
    
    // Start cluster services
    coordinator.start().await?;

    let app_state = Arc::new(RwLock::new(coordinator));

    // Build HTTP router
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/insert", post(insert_data))
        .route("/query/range/:series", get(query_range))
        .route("/query/aggregated/:series", get(query_aggregated))
        .route("/cluster/join", post(handle_cluster_join))
        .route("/cluster/gossip", post(handle_gossip))
        .route("/replicate", post(handle_replication))
        .route("/query/range", post(handle_query_request))
        .route("/query/aggregated", post(handle_aggregated_query_request))
        .layer(
            ServiceBuilder::new()
                .layer(CorsLayer::permissive())
                .into_inner(),
        )
        .with_state(app_state);

    // Start HTTP server
    let listener = tokio::net::TcpListener::bind(&cluster_config.bind_address).await?;
    info!("Server listening on {}", cluster_config.bind_address);

    axum::serve(listener, app).await?;

    Ok(())
}

fn build_cluster_config(count_config: &CountConfig) -> Result<ClusterConfig, Box<dyn std::error::Error>> {
    let node_id = count_config.node_id.ok_or("NODE_ID environment variable is required")?;
    let bind_address: SocketAddr = count_config
        .bind_address
        .as_ref()
        .ok_or("BIND_ADDRESS environment variable is required")?
        .parse()?;

    let seed_nodes: Result<Vec<SocketAddr>, _> = count_config
        .seed_nodes
        .iter()
        .map(|s| s.parse())
        .collect();

    Ok(ClusterConfig {
        node_id,
        bind_address,
        seed_nodes: seed_nodes?,
        replication_factor: count_config.replication_factor,
        virtual_nodes: 150,
    })
}

async fn health_check(State(state): State<AppState>) -> Result<Json<HealthResponse>, StatusCode> {
    let coordinator = state.read().await;
    let status = coordinator.get_cluster_status().await;

    Ok(Json(HealthResponse {
        status: "healthy".to_string(),
        node_id: status.local_node_id,
        cluster_size: status.healthy_nodes,
    }))
}

async fn insert_data(
    State(state): State<AppState>,
    Json(payload): Json<InsertRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let coordinator = state.read().await;
    let series = SeriesKey::from(payload.series);
    let point = DataPoint::new(payload.timestamp, payload.value);

    match coordinator.insert(series, point).await {
        Ok(_) => Ok(StatusCode::CREATED),
        Err(e) => {
            error!("Insert failed: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            ))
        }
    }
}

async fn query_range(
    Path(series): Path<String>,
    Query(params): Query<RangeQueryParams>,
    State(state): State<AppState>,
) -> Result<Json<Vec<DataPoint>>, (StatusCode, Json<ErrorResponse>)> {
    let coordinator = state.read().await;
    let series_key = SeriesKey::from(series);

    match coordinator.query_range(series_key, params.start, params.end).await {
        Ok(data) => Ok(Json(data)),
        Err(e) => {
            error!("Range query failed: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            ))
        }
    }
}

async fn query_aggregated(
    Path(series): Path<String>,
    Query(params): Query<AggregateQueryParams>,
    State(state): State<AppState>,
) -> Result<Json<f64>, (StatusCode, Json<ErrorResponse>)> {
    let coordinator = state.read().await;
    let series_key = SeriesKey::from(series);

    let aggregation = match params.aggregation.as_str() {
        "sum" => AggregationType::Sum,
        "mean" | "avg" => AggregationType::Mean,
        "min" => AggregationType::Min,
        "max" => AggregationType::Max,
        "count" => AggregationType::Count,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Invalid aggregation type".to_string(),
                }),
            ))
        }
    };

    match coordinator.query_aggregated(series_key, params.start, params.end, aggregation).await {
        Ok(result) => Ok(Json(result)),
        Err(e) => {
            error!("Aggregated query failed: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            ))
        }
    }
}

async fn handle_cluster_join(
    State(state): State<AppState>,
    Json(message): Json<GossipMessage>,
) -> Result<Json<GossipMessage>, (StatusCode, Json<ErrorResponse>)> {
    let coordinator = state.read().await;

    match coordinator.handle_gossip_message(message).await {
        Ok(_) => {
            let cluster_state = coordinator.discovery.get_cluster_state().await;
            Ok(Json(cluster_state))
        }
        Err(e) => {
            error!("Cluster join failed: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            ))
        }
    }
}

async fn handle_gossip(
    State(state): State<AppState>,
    Json(message): Json<GossipMessage>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let coordinator = state.read().await;

    match coordinator.handle_gossip_message(message).await {
        Ok(_) => Ok(StatusCode::OK),
        Err(e) => {
            error!("Gossip handling failed: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            ))
        }
    }
}

async fn handle_replication(
    State(state): State<AppState>,
    Json(request): Json<ReplicationRequest>,
) -> Json<count::cluster::replication::ReplicationResponse> {
    let coordinator = state.read().await;
    let response = coordinator.handle_replication_request(request).await;
    Json(response)
}

async fn handle_query_request(
    State(state): State<AppState>,
    Json(request): Json<QueryRequest>,
) -> Json<count::distributed::query_router::QueryResponse> {
    let coordinator = state.read().await;
    let response = coordinator.handle_query_request(request).await;
    Json(response)
}

async fn handle_aggregated_query_request(
    State(state): State<AppState>,
    Json(request): Json<QueryRequest>,
) -> Json<count::distributed::query_router::QueryResponse> {
    let coordinator = state.read().await;
    let response = coordinator.handle_query_request(request).await;
    Json(response)
}