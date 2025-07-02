use tsdb_core::DataPoint;
use crate::error::QueryError;

#[derive(Debug, Clone)]
pub enum Aggregation {
    Sum,
    Avg,
    Min,
    Max,
    Count,
    First,
    Last,
    StdDev,
}

#[derive(Debug, Clone)]
pub struct AggregatedPoint {
    pub timestamp: u64,
    pub value: f64,
    pub count: usize,
}

impl AggregatedPoint {
    pub fn new(timestamp: u64, value: f64, count: usize) -> Self {
        Self { timestamp, value, count }
    }
}

pub fn aggregate_points(
    points: &[DataPoint],
    aggregation: Aggregation,
    window_size: u64,
) -> Result<Vec<AggregatedPoint>, QueryError> {
    if points.is_empty() {
        return Ok(Vec::new());
    }
    
    let mut result = Vec::new();
    let start_time = points[0].timestamp;
    let end_time = points.last().unwrap().timestamp;
    
    let mut current_window_start = start_time;
    
    while current_window_start <= end_time {
        let window_end = current_window_start + window_size;
        
        let window_points: Vec<&DataPoint> = points
            .iter()
            .filter(|p| p.timestamp >= current_window_start && p.timestamp < window_end)
            .collect();
        
        if !window_points.is_empty() {
            let aggregated_value = match aggregation {
                Aggregation::Sum => window_points.iter().map(|p| p.value).sum(),
                Aggregation::Avg => {
                    let sum: f64 = window_points.iter().map(|p| p.value).sum();
                    sum / window_points.len() as f64
                },
                Aggregation::Min => window_points.iter()
                    .map(|p| p.value)
                    .fold(f64::INFINITY, f64::min),
                Aggregation::Max => window_points.iter()
                    .map(|p| p.value)
                    .fold(f64::NEG_INFINITY, f64::max),
                Aggregation::Count => window_points.len() as f64,
                Aggregation::First => window_points[0].value,
                Aggregation::Last => window_points.last().unwrap().value,
                Aggregation::StdDev => {
                    let mean: f64 = window_points.iter().map(|p| p.value).sum::<f64>() 
                        / window_points.len() as f64;
                    let variance: f64 = window_points.iter()
                        .map(|p| (p.value - mean).powi(2))
                        .sum::<f64>() / window_points.len() as f64;
                    variance.sqrt()
                },
            };
            
            result.push(AggregatedPoint::new(
                current_window_start,
                aggregated_value,
                window_points.len(),
            ));
        }
        
        current_window_start = window_end;
    }
    
    Ok(result)
}

pub fn downsample_points(
    points: &[DataPoint],
    max_points: usize,
) -> Vec<DataPoint> {
    if points.len() <= max_points {
        return points.to_vec();
    }
    
    let step = (points.len() as f64 / max_points as f64).ceil() as usize;
    let mut result = Vec::with_capacity(max_points);
    
    for i in (0..points.len()).step_by(step.max(1)) {
        result.push(points[i].clone());
        if result.len() >= max_points {
            break;
        }
    }
    
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_points() -> Vec<DataPoint> {
        vec![
            DataPoint::new(1000, 10.0),
            DataPoint::new(1100, 20.0),
            DataPoint::new(1200, 30.0),
            DataPoint::new(1300, 40.0),
            DataPoint::new(1400, 50.0),
        ]
    }

    #[test]
    fn test_aggregate_sum() {
        let points = create_test_points();
        let result = aggregate_points(&points, Aggregation::Sum, 1000).unwrap();
        
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].value, 150.0); // 10 + 20 + 30 + 40 + 50
        assert_eq!(result[0].count, 5);
    }

    #[test]
    fn test_aggregate_avg() {
        let points = create_test_points();
        let result = aggregate_points(&points, Aggregation::Avg, 200).unwrap();
        
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].value, 15.0); // (10 + 20) / 2
        assert_eq!(result[1].value, 35.0); // (30 + 40) / 2
        assert_eq!(result[2].value, 50.0); // 50 / 1
    }

    #[test]
    fn test_aggregate_min_max() {
        let points = create_test_points();
        
        let min_result = aggregate_points(&points, Aggregation::Min, 300).unwrap();
        let max_result = aggregate_points(&points, Aggregation::Max, 300).unwrap();
        
        assert_eq!(min_result[0].value, 10.0);
        assert_eq!(max_result[0].value, 30.0);
    }

    #[test]
    fn test_aggregate_count() {
        let points = create_test_points();
        let result = aggregate_points(&points, Aggregation::Count, 250).unwrap();
        
        assert_eq!(result[0].value, 3.0); // Points 1000, 1100, 1200
        assert_eq!(result[1].value, 2.0); // Points 1300, 1400
    }

    #[test]
    fn test_aggregate_first_last() {
        let points = create_test_points();
        
        let first_result = aggregate_points(&points, Aggregation::First, 200).unwrap();
        let last_result = aggregate_points(&points, Aggregation::Last, 200).unwrap();
        
        assert_eq!(first_result[0].value, 10.0);
        assert_eq!(last_result[0].value, 20.0);
    }

    #[test]
    fn test_aggregate_empty_points() {
        let points = Vec::new();
        let result = aggregate_points(&points, Aggregation::Sum, 100).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_downsample_no_change() {
        let points = create_test_points();
        let result = downsample_points(&points, 10);
        assert_eq!(result.len(), 5);
    }

    #[test]
    fn test_downsample_reduce() {
        let points = create_test_points(); // 5 points: 1000, 1100, 1200, 1300, 1400
        let result = downsample_points(&points, 3);
        // Step should be ceil(5/3) = 2, so we get indices 0, 2, 4
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].timestamp, 1000); // index 0
        assert_eq!(result[1].timestamp, 1200); // index 2  
        assert_eq!(result[2].timestamp, 1400); // index 4
    }
}