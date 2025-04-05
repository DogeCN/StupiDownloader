pub const AGENT: (&str, &str) = ("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/111.0.0.0 Safari/537.36");

pub const KB: u64 = 1024;
pub const MB: u64 = 1_048_576;
pub const GB: u64 = 1_073_741_824;
pub const SIZE_TABLE: [(u64, u64); 12] = [
    (128 * KB, 1),    // 128KB
    (512 * KB, 4),    // 512KB
    (MB, 8),          // 1MB
    (5 * MB, 32),     // 5MB
    (10 * MB, 64),    // 10MB
    (50 * MB, 256),   // 50MB
    (100 * MB, 384),  // 100MB
    (500 * MB, 512),  // 500MB
    (GB, 640),        // 1GB
    (5 * GB, 768),    // 5GB
    (10 * GB, 869),   // 10GB
    (u64::MAX, 1024), // 10+GB
];
