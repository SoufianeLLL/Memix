//! Storage compression for reducing brain size.
//! Uses zstd for fast compression with good ratios.

use crate::brain::schema::MemoryEntry;
use crate::error::{MemixError, Result};

/// Compress a memory entry to bytes.
/// Uses zstd level 3 for good balance of speed and compression.
pub fn compress_entry(entry: &MemoryEntry) -> Result<Vec<u8>> {
    let json = serde_json::to_vec(entry)?;
    let compressed = zstd::encode_all(&json[..], 3)
        .map_err(|e| MemixError::database(format!("Compression failed: {}", e), false))?;
    Ok(compressed)
}

/// Decompress bytes back to a memory entry.
pub fn decompress_entry(data: &[u8]) -> Result<MemoryEntry> {
    let json = zstd::decode_all(data)
        .map_err(|e| MemixError::database(format!("Decompression failed: {}", e), false))?;
    let entry = serde_json::from_slice(&json)?;
    Ok(entry)
}

/// Compress multiple entries at once.
/// Returns a map of entry ID -> compressed bytes.
pub fn compress_entries(entries: &[MemoryEntry]) -> Result<Vec<(String, Vec<u8>)>> {
    entries
        .iter()
        .map(|entry| {
            let compressed = compress_entry(entry)?;
            Ok((entry.id.clone(), compressed))
        })
        .collect()
}

/// Calculate compression ratio for an entry.
/// Returns (original_size, compressed_size, ratio).
pub fn compression_ratio(entry: &MemoryEntry) -> Result<(usize, usize, f64)> {
    let json = serde_json::to_vec(entry)?;
    let compressed = compress_entry(entry)?;
    let ratio = json.len() as f64 / compressed.len() as f64;
    Ok((json.len(), compressed.len(), ratio))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MemoryKind, MemorySource};
    use chrono::Utc;

    fn make_test_entry() -> MemoryEntry {
        MemoryEntry {
            id: "test:entry".to_string(),
            project_id: "test-project".to_string(),
            kind: MemoryKind::Fact,
            content: r#"{"name":"test","value":"This is a test entry with some content that should compress well when repeated. This is a test entry with some content that should compress well when repeated."}"#.to_string(),
            tags: vec!["test".to_string()],
            source: MemorySource::UserManual,
            superseded_by: None,
            contradicts: vec![],
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
            access_count: 0,
            last_accessed_at: None,
        }
    }

    #[test]
    fn test_compress_decompress() {
        let entry = make_test_entry();
        let compressed = compress_entry(&entry).unwrap();
        let decompressed = decompress_entry(&compressed).unwrap();
        
        assert_eq!(entry.id, decompressed.id);
        assert_eq!(entry.content, decompressed.content);
    }

    #[test]
    fn test_compression_ratio() {
        let entry = make_test_entry();
        let (original, compressed, ratio) = compression_ratio(&entry).unwrap();
        
        println!("Original: {} bytes", original);
        println!("Compressed: {} bytes", compressed);
        println!("Ratio: {:.2}x", ratio);
        
        assert!(compressed < original);
        assert!(ratio > 1.0);
    }
}
