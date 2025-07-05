use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AggregationType {
    Mean,
    Sum,
    Min,
    Max,
    Count,
}

#[derive(Debug, Clone)]
pub struct QueryRequest {
    pub series_key: String,
    pub start_time: u64,
    pub end_time: u64,
    pub aggregation: Option<AggregationType>,
    pub step_ms: Option<u64>, // For downsampling
}

#[derive(Debug, Clone)]
pub struct QueryResult {
    pub series_key: String,
    pub timestamps: Vec<u64>,
    pub values: Vec<f64>,
    pub aggregation_type: Option<AggregationType>,
}

impl QueryResult {
    pub fn new(series_key: String) -> Self {
        Self {
            series_key,
            timestamps: Vec::new(),
            values: Vec::new(),
            aggregation_type: None,
        }
    }

    pub fn with_aggregation(mut self, agg_type: AggregationType) -> Self {
        self.aggregation_type = Some(agg_type);
        self
    }

    pub fn add_point(&mut self, timestamp: u64, value: f64) {
        self.timestamps.push(timestamp);
        self.values.push(value);
    }

    pub fn len(&self) -> usize {
        self.timestamps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.timestamps.is_empty()
    }
}