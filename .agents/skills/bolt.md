
## Performance Optimization
* ⚡ **N+1 Async Event Loading**: In `src/constellations/handlers.rs`, fixed an N+1 async I/O anti-pattern when loading pinned event details. A sequential `for` loop `for id in ids { matrix.fetch_pinned_event_details(..).await }` was updated to concurrently fetch using `futures::future::join_all(ids.into_iter().map(..))`. This parallelization significantly reduces the overall I/O waiting time when rendering pinned events for large channels.
