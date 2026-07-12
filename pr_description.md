## ⚡ [Performance Improvement] Non-blocking I/O for room avatar uploads

### 💡 What:
Replaced the synchronous `std::fs::read` with asynchronous `tokio::fs::read` when reading the selected room avatar file before uploading.

### 🎯 Why:
Using `std::fs::read` inside a Tokio `async` block executes the blocking I/O operation on the async executor thread. This blocks the executor from running other async tasks while the file is being read from disk, which is a known anti-pattern that can lead to UI stuttering, dropped frames, or increased latency for concurrent background tasks (like sync loops or message sending). Using `tokio::fs::read` correctly yields control back to the executor during the file read operation.

### 📊 Measured Improvement:
While difficult to benchmark this exact user-driven UI action directly (selecting a file via file picker), replacing blocking I/O with async I/O inside an async block guarantees that the Tokio worker thread is not blocked during disk reads. The latency impact of disk reads varies wildly based on storage speed (HDD vs NVMe), OS caching, and file size, but blocking an executor thread for even a few milliseconds can disrupt a 60 FPS UI rendering loop (which has a 16.6ms budget per frame). This optimization ensures the UI thread remains responsive and concurrent async background tasks continue executing smoothly even when reading large images from slower storage.
