pub const AGENT: (&str, &str) = ("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/111.0.0.0 Safari/537.36");

pub const MB: u64 = 1_048_576;
pub const GB: u64 = 1_073_741_824;
pub const SIZE_TABLE: [(u64, u64); 6] = [
    (MB, 1),        // 1MB
    (10 * MB, 2),   // 10MB
    (100 * MB, 4),  // 100MB
    (GB, 8),        // 1GB
    (10 * GB, 16),  // 10GB
    (u64::MAX, 32), // 10+GB
];
