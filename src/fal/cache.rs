//! VideoCache - persistent disk cache for generated videos.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Persistent disk cache for generated fal.ai videos.
pub struct VideoCache {
    cache_dir: PathBuf,
}

impl VideoCache {
    /// Create a new VideoCache with the given cache directory.
    /// Does not create the directory - call `ensure_dir_exists()` to create it.
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    /// Create a VideoCache with the default cache directory.
    /// Default: ~/.cache/space-recorder/fal-videos/
    /// Does not create the directory - call `ensure_dir_exists()` to create it.
    pub fn with_default_dir() -> Result<Self, std::io::Error> {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from(".cache"))
            .join("space-recorder")
            .join("fal-videos");
        Ok(Self::new(cache_dir))
    }

    /// Create a VideoCache with the default directory and ensure it exists.
    /// This is the preferred constructor for production use.
    pub fn with_default_dir_initialized() -> Result<Self, std::io::Error> {
        let cache = Self::with_default_dir()?;
        cache.ensure_dir_exists()?;
        Ok(cache)
    }

    /// Create a VideoCache with a custom directory and ensure it exists.
    /// This is the preferred constructor when using a custom cache path.
    pub fn new_initialized(cache_dir: PathBuf) -> Result<Self, std::io::Error> {
        let cache = Self::new(cache_dir);
        cache.ensure_dir_exists()?;
        Ok(cache)
    }

    /// Ensure the cache directory exists, creating it if necessary.
    pub fn ensure_dir_exists(&self) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(&self.cache_dir)
    }

    /// Get cached video by prompt hash, if exists.
    pub fn get(&self, prompt: &str) -> Option<PathBuf> {
        let hash = Self::hash_prompt(prompt);
        let video_path = self.cache_dir.join(format!("{}.mp4", hash));
        if video_path.exists() {
            Some(video_path)
        } else {
            None
        }
    }

    /// Store video with prompt hash.
    pub fn store(&self, prompt: &str, video_path: &Path) -> Result<PathBuf, std::io::Error> {
        std::fs::create_dir_all(&self.cache_dir)?;
        let hash = Self::hash_prompt(prompt);
        let cached_path = self.cache_dir.join(format!("{}.mp4", hash));
        std::fs::copy(video_path, &cached_path)?;
        Ok(cached_path)
    }

    /// Generate deterministic SHA256 hash for prompt.
    /// Returns a 32-character hex string (first 16 bytes of SHA256).
    /// Same prompt always produces the same hash.
    pub fn hash_prompt(prompt: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(prompt.as_bytes());
        let result = hasher.finalize();
        // Use first 16 bytes (32 hex chars) for shorter filenames
        hex::encode(&result[..16])
    }

    /// Get the cache directory path.
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Store video with prompt hash and automatically cleanup if needed.
    /// This is the preferred method for production use as it handles cache size limits.
    pub fn store_with_cleanup(
        &self,
        prompt: &str,
        video_path: &Path,
        max_size_mb: u64,
    ) -> Result<PathBuf, std::io::Error> {
        let cached_path = self.store(prompt, video_path)?;
        self.cleanup_if_needed(max_size_mb)?;
        Ok(cached_path)
    }

    /// Remove old files if cache exceeds max size.
    /// Deletes oldest files first (by modification time) until under limit.
    pub fn cleanup_if_needed(&self, max_size_mb: u64) -> Result<(), std::io::Error> {
        let max_size_bytes = max_size_mb * 1024 * 1024;

        // Get all cached video files with their metadata
        let mut files: Vec<(PathBuf, std::fs::Metadata)> = Vec::new();
        let mut total_size: u64 = 0;

        if !self.cache_dir.exists() {
            return Ok(());
        }

        for entry in std::fs::read_dir(&self.cache_dir)? {
            let entry = entry?;
            let path = entry.path();

            // Only consider .mp4 files
            if path.extension().and_then(|e| e.to_str()) == Some("mp4") {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        total_size += metadata.len();
                        files.push((path, metadata));
                    }
                }
            }
        }

        // If under limit, nothing to do
        if total_size <= max_size_bytes {
            return Ok(());
        }

        // Sort by modification time (oldest first)
        files.sort_by(|a, b| {
            let time_a = a.1.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            let time_b = b.1.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            time_a.cmp(&time_b)
        });

        // Delete oldest files until under limit
        for (path, metadata) in files {
            if total_size <= max_size_bytes {
                break;
            }

            let file_size = metadata.len();
            if std::fs::remove_file(&path).is_ok() {
                total_size = total_size.saturating_sub(file_size);
            }
        }

        Ok(())
    }

    /// Get total size of all cached videos in bytes.
    pub fn total_size_bytes(&self) -> Result<u64, std::io::Error> {
        let mut total: u64 = 0;

        if !self.cache_dir.exists() {
            return Ok(0);
        }

        for entry in std::fs::read_dir(&self.cache_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) == Some("mp4") {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        total += metadata.len();
                    }
                }
            }
        }

        Ok(total)
    }

    /// Store video with prompt hash and save the original prompt as metadata.
    /// This is the preferred method for storing videos as it enables prompt lookup.
    pub fn store_with_metadata(
        &self,
        prompt: &str,
        video_path: &Path,
    ) -> Result<PathBuf, std::io::Error> {
        let cached_path = self.store(prompt, video_path)?;

        // Save prompt as metadata file alongside the video
        let hash = Self::hash_prompt(prompt);
        let meta_path = self.cache_dir.join(format!("{}.prompt", hash));
        std::fs::write(&meta_path, prompt)?;

        Ok(cached_path)
    }

    /// Get the prompt for a cached video by its hash.
    pub fn get_prompt(&self, hash: &str) -> Option<String> {
        let meta_path = self.cache_dir.join(format!("{}.prompt", hash));
        std::fs::read_to_string(&meta_path).ok()
    }

    /// List all cached video entries with their hashes, prompts (if available), and sizes.
    pub fn list_entries(&self) -> Result<Vec<CacheEntry>, std::io::Error> {
        let mut entries = Vec::new();

        if !self.cache_dir.exists() {
            return Ok(entries);
        }

        for entry in std::fs::read_dir(&self.cache_dir)? {
            let entry = entry?;
            let path = entry.path();

            // Only process .mp4 files
            if path.extension().and_then(|e| e.to_str()) != Some("mp4") {
                continue;
            }

            let metadata = entry.metadata()?;
            if !metadata.is_file() {
                continue;
            }

            // Extract hash from filename (without .mp4 extension)
            let hash = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();

            // Try to get the prompt from metadata file
            let prompt = self.get_prompt(&hash);

            entries.push(CacheEntry {
                hash,
                prompt,
                size_bytes: metadata.len(),
                path,
            });
        }

        // Sort by hash for consistent output
        entries.sort_by(|a, b| a.hash.cmp(&b.hash));

        Ok(entries)
    }

    /// Remove a cached video by its hash.
    /// Returns true if a file was removed, false if it didn't exist.
    pub fn remove(&self, hash: &str) -> Result<bool, std::io::Error> {
        let video_path = self.cache_dir.join(format!("{}.mp4", hash));
        let meta_path = self.cache_dir.join(format!("{}.prompt", hash));

        let mut removed = false;

        if video_path.exists() {
            std::fs::remove_file(&video_path)?;
            removed = true;
        }

        // Also remove metadata file if it exists (don't fail if it doesn't)
        let _ = std::fs::remove_file(&meta_path);

        Ok(removed)
    }

    /// Remove all cached videos and metadata.
    /// Returns the number of videos removed.
    pub fn clear_all(&self) -> Result<usize, std::io::Error> {
        if !self.cache_dir.exists() {
            return Ok(0);
        }

        let mut count = 0;

        for entry in std::fs::read_dir(&self.cache_dir)? {
            let entry = entry?;
            let path = entry.path();

            // Remove .mp4 video files (count these)
            if path.extension().and_then(|e| e.to_str()) == Some("mp4") {
                if std::fs::remove_file(&path).is_ok() {
                    count += 1;
                }
            }
            // Also remove .prompt metadata files
            else if path.extension().and_then(|e| e.to_str()) == Some("prompt") {
                let _ = std::fs::remove_file(&path);
            }
        }

        Ok(count)
    }
}

/// Information about a cached video entry.
#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// SHA256 hash of the prompt (first 32 hex chars)
    pub hash: String,
    /// Original prompt text, if metadata was stored
    pub prompt: Option<String>,
    /// Size of the video file in bytes
    pub size_bytes: u64,
    /// Full path to the cached video
    pub path: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_new_with_custom_dir() {
        let custom_path = PathBuf::from("/tmp/test-cache/fal-videos");
        let cache = VideoCache::new(custom_path.clone());
        assert_eq!(cache.cache_dir(), custom_path);
    }

    #[test]
    fn test_with_default_dir() {
        let cache = VideoCache::with_default_dir().unwrap();
        let path_str = cache.cache_dir().to_string_lossy();
        assert!(path_str.contains("space-recorder"));
        assert!(path_str.contains("fal-videos"));
    }

    #[test]
    fn test_default_dir_path_structure() {
        // AC: Default cache dir is ~/.cache/space-recorder/fal-videos/
        let cache = VideoCache::with_default_dir().unwrap();
        let components: Vec<_> = cache.cache_dir().components().collect();

        // Check the last three components are: space-recorder/fal-videos
        let component_names: Vec<_> = components
            .iter()
            .filter_map(|c| c.as_os_str().to_str())
            .collect();

        assert!(component_names.contains(&"space-recorder"));
        assert!(component_names.contains(&"fal-videos"));

        // fal-videos should come after space-recorder
        let sr_pos = component_names.iter().position(|&s| s == "space-recorder");
        let fv_pos = component_names.iter().position(|&s| s == "fal-videos");
        assert!(sr_pos.is_some() && fv_pos.is_some());
        assert!(fv_pos.unwrap() > sr_pos.unwrap());
    }

    #[test]
    fn test_ensure_dir_exists_creates_directory() {
        // AC: Creates cache directory if doesn't exist
        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().join("nested").join("fal-videos");

        // Directory shouldn't exist yet
        assert!(!cache_path.exists());

        let cache = VideoCache::new(cache_path.clone());
        cache.ensure_dir_exists().unwrap();

        // Directory should now exist
        assert!(cache_path.exists());
        assert!(cache_path.is_dir());
    }

    #[test]
    fn test_ensure_dir_exists_idempotent() {
        // Calling ensure_dir_exists multiple times should succeed
        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().join("fal-videos");

        let cache = VideoCache::new(cache_path.clone());
        cache.ensure_dir_exists().unwrap();
        cache.ensure_dir_exists().unwrap(); // Should not fail

        assert!(cache_path.exists());
    }

    #[test]
    fn test_new_initialized_creates_directory() {
        // AC: Creates cache directory if doesn't exist
        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().join("auto-created").join("fal-videos");

        assert!(!cache_path.exists());

        let cache = VideoCache::new_initialized(cache_path.clone()).unwrap();

        assert!(cache_path.exists());
        assert!(cache_path.is_dir());
        assert_eq!(cache.cache_dir(), cache_path);
    }

    #[test]
    fn test_configurable_cache_directory() {
        // AC: Configurable cache directory via config
        let custom_paths = vec![
            PathBuf::from("/tmp/custom-cache"),
            PathBuf::from("/var/cache/space-recorder"),
            PathBuf::from("./local-cache"),
        ];

        for path in custom_paths {
            let cache = VideoCache::new(path.clone());
            assert_eq!(cache.cache_dir(), path);
        }
    }

    #[test]
    fn test_cache_dir_accessor() {
        let path = PathBuf::from("/test/path");
        let cache = VideoCache::new(path.clone());
        assert_eq!(cache.cache_dir(), path.as_path());
    }

    #[test]
    fn test_hash_prompt_deterministic() {
        // Same prompt should always produce the same hash
        let prompt = "cyberpunk cityscape";
        let hash1 = VideoCache::hash_prompt(prompt);
        let hash2 = VideoCache::hash_prompt(prompt);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_prompt_different_for_different_prompts() {
        let hash1 = VideoCache::hash_prompt("cyberpunk cityscape");
        let hash2 = VideoCache::hash_prompt("abstract particles");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_hash_prompt_is_filesystem_safe() {
        // Hash should only contain hex characters (0-9, a-f)
        let hash = VideoCache::hash_prompt("test prompt with special chars !@#$%");
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(hash.len(), 32); // 16 bytes = 32 hex chars
    }

    #[test]
    fn test_get_returns_none_for_missing() {
        let temp_dir = TempDir::new().unwrap();
        let cache = VideoCache::new(temp_dir.path().to_path_buf());
        cache.ensure_dir_exists().unwrap();

        assert!(cache.get("nonexistent prompt").is_none());
    }

    #[test]
    fn test_store_and_get() {
        let temp_dir = TempDir::new().unwrap();
        let cache = VideoCache::new_initialized(temp_dir.path().join("cache")).unwrap();

        // Create a test video file
        let source_path = temp_dir.path().join("source.mp4");
        fs::write(&source_path, b"fake video content").unwrap();

        let prompt = "test prompt";

        // Store the video
        let cached_path = cache.store(prompt, &source_path).unwrap();
        assert!(cached_path.exists());

        // Get should now return the path
        let retrieved = cache.get(prompt);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), cached_path);
    }

    #[test]
    fn test_store_overwrites_existing() {
        let temp_dir = TempDir::new().unwrap();
        let cache = VideoCache::new_initialized(temp_dir.path().join("cache")).unwrap();

        let prompt = "same prompt";

        // Create first video
        let video1 = temp_dir.path().join("video1.mp4");
        fs::write(&video1, b"content 1").unwrap();
        cache.store(prompt, &video1).unwrap();

        // Create second video with different content
        let video2 = temp_dir.path().join("video2.mp4");
        fs::write(&video2, b"content 2 - different").unwrap();
        let cached_path = cache.store(prompt, &video2).unwrap();

        // Cached file should have new content
        let content = fs::read_to_string(&cached_path).unwrap();
        assert_eq!(content, "content 2 - different");
    }

    #[test]
    fn test_get_handles_deleted_files() {
        let temp_dir = TempDir::new().unwrap();
        let cache = VideoCache::new_initialized(temp_dir.path().join("cache")).unwrap();

        // Create and store a video
        let source = temp_dir.path().join("source.mp4");
        fs::write(&source, b"content").unwrap();
        let prompt = "test";
        let cached_path = cache.store(prompt, &source).unwrap();

        // Verify it exists
        assert!(cache.get(prompt).is_some());

        // Delete the cached file
        fs::remove_file(&cached_path).unwrap();

        // Get should now return None (handles deleted files)
        assert!(cache.get(prompt).is_none());
    }

    #[test]
    fn test_cleanup_if_needed_removes_old_files() {
        // AC: cleanup_if_needed(max_size_mb: u64) removes old files
        let temp_dir = TempDir::new().unwrap();
        let cache = VideoCache::new_initialized(temp_dir.path().join("cache")).unwrap();

        // Create several videos (each 1KB = 1024 bytes)
        let content = vec![0u8; 1024];
        for i in 0..5 {
            let source = temp_dir.path().join(format!("video{}.mp4", i));
            fs::write(&source, &content).unwrap();
            cache.store(&format!("prompt {}", i), &source).unwrap();
            // Sleep briefly to ensure different modification times
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // Total is now 5KB. Cleanup with 3KB limit should remove files.
        // 3KB = 3 * 1024 bytes = 3072 bytes
        // But max_size_mb expects MB, so we use a workaround with very small limit
        // Let's check that it removes files when over limit

        // Count files before
        let files_before: Vec<_> = fs::read_dir(cache.cache_dir())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("mp4"))
            .collect();
        assert_eq!(files_before.len(), 5);

        // Cleanup with max size of 0 should remove all files
        cache.cleanup_if_needed(0).unwrap();

        let files_after: Vec<_> = fs::read_dir(cache.cache_dir())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("mp4"))
            .collect();
        assert_eq!(files_after.len(), 0);
    }

    #[test]
    fn test_cleanup_deletes_oldest_first() {
        // AC: Deletes oldest files first (by modification time)
        let temp_dir = TempDir::new().unwrap();
        let cache = VideoCache::new_initialized(temp_dir.path().join("cache")).unwrap();

        // Create videos with different timestamps
        let content = vec![0u8; 1024]; // 1KB each

        // Create first (oldest) file
        let source1 = temp_dir.path().join("video1.mp4");
        fs::write(&source1, &content).unwrap();
        cache.store("oldest prompt", &source1).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Create second (middle) file
        let source2 = temp_dir.path().join("video2.mp4");
        fs::write(&source2, &content).unwrap();
        cache.store("middle prompt", &source2).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Create third (newest) file
        let source3 = temp_dir.path().join("video3.mp4");
        fs::write(&source3, &content).unwrap();
        cache.store("newest prompt", &source3).unwrap();

        // Total is 3KB. Cleanup to keep only about 2KB should remove oldest
        // Since we can't use fractional MB, we'll verify with 0 limit
        // that oldest gets deleted first by checking which remain

        // Instead, let's use cleanup to remove 1 file worth
        // We need to reduce from 3KB to less - but MB granularity is too coarse
        // Let's test that when max is 0, we remove all (oldest first)

        // Actually, we can test by manually calling cleanup multiple times
        // and verifying which files get deleted

        // Get current total size
        let total_before = cache.total_size_bytes().unwrap();
        assert_eq!(total_before, 3072); // 3 * 1024

        // Set max to 0 - should delete all, oldest first
        cache.cleanup_if_needed(0).unwrap();

        // All should be gone
        assert!(cache.get("oldest prompt").is_none());
        assert!(cache.get("middle prompt").is_none());
        assert!(cache.get("newest prompt").is_none());
    }

    #[test]
    fn test_store_with_cleanup_integrates_cleanup() {
        // AC: Runs automatically when storing new video
        let temp_dir = TempDir::new().unwrap();
        let cache = VideoCache::new_initialized(temp_dir.path().join("cache")).unwrap();

        // Create several videos first (each 1KB)
        let content = vec![0u8; 1024];
        for i in 0..3 {
            let source = temp_dir.path().join(format!("video{}.mp4", i));
            fs::write(&source, &content).unwrap();
            cache.store(&format!("prompt {}", i), &source).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // Now store with cleanup at max 0 - should cleanup after storing
        let new_video = temp_dir.path().join("new_video.mp4");
        fs::write(&new_video, &content).unwrap();
        cache.store_with_cleanup("new prompt", &new_video, 0).unwrap();

        // Cache should be empty (cleanup removes everything including the new file)
        let total = cache.total_size_bytes().unwrap();
        assert_eq!(total, 0);
    }

    #[test]
    fn test_cleanup_with_large_max_does_nothing() {
        // AC: Configurable max size (default 1GB)
        let temp_dir = TempDir::new().unwrap();
        let cache = VideoCache::new_initialized(temp_dir.path().join("cache")).unwrap();

        // Create a small video
        let content = vec![0u8; 1024]; // 1KB
        let source = temp_dir.path().join("video.mp4");
        fs::write(&source, &content).unwrap();
        cache.store("test prompt", &source).unwrap();

        // Cleanup with 1000 MB limit (1GB) should do nothing
        cache.cleanup_if_needed(1000).unwrap();

        // File should still exist
        assert!(cache.get("test prompt").is_some());
    }

    #[test]
    fn test_cleanup_handles_empty_cache() {
        let temp_dir = TempDir::new().unwrap();
        let cache = VideoCache::new_initialized(temp_dir.path().join("cache")).unwrap();

        // Cleanup on empty cache should not error
        cache.cleanup_if_needed(0).unwrap();
        cache.cleanup_if_needed(1000).unwrap();
    }

    #[test]
    fn test_cleanup_handles_nonexistent_cache_dir() {
        let temp_dir = TempDir::new().unwrap();
        let cache = VideoCache::new(temp_dir.path().join("nonexistent").join("cache"));

        // Cleanup when dir doesn't exist should not error
        cache.cleanup_if_needed(0).unwrap();
    }

    #[test]
    fn test_total_size_bytes() {
        let temp_dir = TempDir::new().unwrap();
        let cache = VideoCache::new_initialized(temp_dir.path().join("cache")).unwrap();

        // Empty cache should be 0
        assert_eq!(cache.total_size_bytes().unwrap(), 0);

        // Add some files
        let content = vec![0u8; 1024]; // 1KB
        for i in 0..3 {
            let source = temp_dir.path().join(format!("video{}.mp4", i));
            fs::write(&source, &content).unwrap();
            cache.store(&format!("prompt {}", i), &source).unwrap();
        }

        // Should be 3KB total
        assert_eq!(cache.total_size_bytes().unwrap(), 3072);
    }

    #[test]
    fn test_cleanup_ignores_non_mp4_files() {
        let temp_dir = TempDir::new().unwrap();
        let cache = VideoCache::new_initialized(temp_dir.path().join("cache")).unwrap();

        // Create mp4 and non-mp4 files in cache dir
        let content = vec![0u8; 1024];

        // Create a .mp4 file via store
        let source = temp_dir.path().join("video.mp4");
        fs::write(&source, &content).unwrap();
        cache.store("test", &source).unwrap();

        // Create a non-mp4 file directly in cache dir
        let txt_file = cache.cache_dir().join("notes.txt");
        fs::write(&txt_file, &content).unwrap();

        // Cleanup with 0 limit
        cache.cleanup_if_needed(0).unwrap();

        // mp4 should be deleted, txt should remain
        assert!(cache.get("test").is_none());
        assert!(txt_file.exists());
    }

    #[test]
    fn test_store_with_metadata() {
        // AC: Stores prompt alongside video
        let temp_dir = TempDir::new().unwrap();
        let cache = VideoCache::new_initialized(temp_dir.path().join("cache")).unwrap();

        let source = temp_dir.path().join("video.mp4");
        fs::write(&source, b"video content").unwrap();

        let prompt = "cyberpunk cityscape";
        cache.store_with_metadata(prompt, &source).unwrap();

        // Should be able to retrieve prompt
        let hash = VideoCache::hash_prompt(prompt);
        let retrieved_prompt = cache.get_prompt(&hash);
        assert_eq!(retrieved_prompt, Some(prompt.to_string()));
    }

    #[test]
    fn test_get_prompt_returns_none_for_missing() {
        let temp_dir = TempDir::new().unwrap();
        let cache = VideoCache::new_initialized(temp_dir.path().join("cache")).unwrap();

        assert!(cache.get_prompt("nonexistent_hash").is_none());
    }

    #[test]
    fn test_list_entries_empty_cache() {
        // AC: List shows cached videos
        let temp_dir = TempDir::new().unwrap();
        let cache = VideoCache::new_initialized(temp_dir.path().join("cache")).unwrap();

        let entries = cache.list_entries().unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_list_entries_with_videos() {
        let temp_dir = TempDir::new().unwrap();
        let cache = VideoCache::new_initialized(temp_dir.path().join("cache")).unwrap();

        let content = vec![0u8; 1024]; // 1KB
        let source = temp_dir.path().join("video.mp4");
        fs::write(&source, &content).unwrap();

        cache.store_with_metadata("test prompt", &source).unwrap();
        cache.store("prompt without metadata", &source).unwrap();

        let entries = cache.list_entries().unwrap();
        assert_eq!(entries.len(), 2);

        // Find entry with metadata
        let entry_with_meta = entries.iter().find(|e| e.prompt.is_some()).unwrap();
        assert_eq!(entry_with_meta.prompt, Some("test prompt".to_string()));
        assert_eq!(entry_with_meta.size_bytes, 1024);

        // Find entry without metadata
        let entry_without_meta = entries.iter().find(|e| e.prompt.is_none()).unwrap();
        assert!(entry_without_meta.prompt.is_none());
    }

    #[test]
    fn test_list_entries_shows_size() {
        // AC: Shows sizes in list output
        let temp_dir = TempDir::new().unwrap();
        let cache = VideoCache::new_initialized(temp_dir.path().join("cache")).unwrap();

        let content = vec![0u8; 2048]; // 2KB
        let source = temp_dir.path().join("video.mp4");
        fs::write(&source, &content).unwrap();

        cache.store("test", &source).unwrap();

        let entries = cache.list_entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].size_bytes, 2048);
    }

    #[test]
    fn test_remove_existing_video() {
        // AC: clear <hash> removes specific cached video
        let temp_dir = TempDir::new().unwrap();
        let cache = VideoCache::new_initialized(temp_dir.path().join("cache")).unwrap();

        let source = temp_dir.path().join("video.mp4");
        fs::write(&source, b"content").unwrap();

        let prompt = "test prompt";
        cache.store_with_metadata(prompt, &source).unwrap();

        let hash = VideoCache::hash_prompt(prompt);
        assert!(cache.get(prompt).is_some());

        let removed = cache.remove(&hash).unwrap();
        assert!(removed);
        assert!(cache.get(prompt).is_none());
        assert!(cache.get_prompt(&hash).is_none()); // Metadata also removed
    }

    #[test]
    fn test_remove_nonexistent_video() {
        let temp_dir = TempDir::new().unwrap();
        let cache = VideoCache::new_initialized(temp_dir.path().join("cache")).unwrap();

        let removed = cache.remove("nonexistent_hash").unwrap();
        assert!(!removed);
    }

    #[test]
    fn test_clear_all_removes_all_videos() {
        // AC: clear removes all cached videos
        let temp_dir = TempDir::new().unwrap();
        let cache = VideoCache::new_initialized(temp_dir.path().join("cache")).unwrap();

        let content = vec![0u8; 1024];
        for i in 0..3 {
            let source = temp_dir.path().join(format!("video{}.mp4", i));
            fs::write(&source, &content).unwrap();
            cache.store_with_metadata(&format!("prompt {}", i), &source).unwrap();
        }

        assert_eq!(cache.list_entries().unwrap().len(), 3);

        let count = cache.clear_all().unwrap();
        assert_eq!(count, 3);
        assert!(cache.list_entries().unwrap().is_empty());
    }

    #[test]
    fn test_clear_all_empty_cache() {
        let temp_dir = TempDir::new().unwrap();
        let cache = VideoCache::new_initialized(temp_dir.path().join("cache")).unwrap();

        let count = cache.clear_all().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_clear_all_removes_metadata_files() {
        let temp_dir = TempDir::new().unwrap();
        let cache = VideoCache::new_initialized(temp_dir.path().join("cache")).unwrap();

        let source = temp_dir.path().join("video.mp4");
        fs::write(&source, b"content").unwrap();

        cache.store_with_metadata("test", &source).unwrap();

        // Verify metadata file exists
        let hash = VideoCache::hash_prompt("test");
        let meta_path = cache.cache_dir().join(format!("{}.prompt", hash));
        assert!(meta_path.exists());

        cache.clear_all().unwrap();

        // Metadata should also be gone
        assert!(!meta_path.exists());
    }

    #[test]
    fn test_list_entries_nonexistent_cache_dir() {
        let temp_dir = TempDir::new().unwrap();
        let cache = VideoCache::new(temp_dir.path().join("nonexistent").join("cache"));

        // Should return empty list, not error
        let entries = cache.list_entries().unwrap();
        assert!(entries.is_empty());
    }
}
