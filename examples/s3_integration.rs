use count::CountConfig;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("S3 Integration Example");
    
    // Example configuration with S3 enabled
    let mut config = CountConfig::default();
    config.s3_enabled = true;
    config.s3_bucket = Some("my-count-db-bucket".to_string());
    config.s3_region = Some("us-east-1".to_string());
    config.s3_prefix = Some("count-data".to_string());
    config.archival_age_days = 14; // Archive data older than 2 weeks
    
    println!("Configuration:");
    println!("  S3 Enabled: {}", config.s3_enabled);
    println!("  S3 Bucket: {:?}", config.s3_bucket);
    println!("  S3 Region: {:?}", config.s3_region);
    println!("  Archival Age: {} days", config.archival_age_days);
    
    // Note: This would require actual AWS credentials and an S3 bucket to run
    println!("Example configuration created successfully!");
    println!("\nTo use S3 integration, set these environment variables:");
    println!("  S3_ENABLED=true");
    println!("  S3_BUCKET=your-bucket-name");
    println!("  S3_REGION=your-aws-region");
    println!("  S3_PREFIX=count-data (optional)");
    println!("  ARCHIVAL_AGE_DAYS=14 (optional, defaults to 14)");
    
    println!("\nThe system will:");
    println!("  1. Store recent data (< 2 weeks) in memory and local disk");
    println!("  2. Automatically migrate older data to S3 every hour");
    println!("  3. Query from all storage tiers (memory, disk, S3) transparently");
    
    Ok(())
}