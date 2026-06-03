use url::Url;

// ⚡ Bolt Optimization: Fast path for ASCII string filtering
// Avoids costly heap allocations from `.to_lowercase()`
pub fn contains_ignore_ascii_case(
    haystack: &str,
    query: &str,
    query_lower_fallback: Option<&str>,
) -> bool {
    if query.is_empty() {
        return true;
    }

    if query.is_ascii() {
        let query_bytes = query.as_bytes();
        let query_len = query_bytes.len();
        let h_bytes = haystack.as_bytes();

        if h_bytes.len() < query_len {
            return false;
        }

        let first_char = query_bytes[0];
        let first_lower = first_char.to_ascii_lowercase();
        let first_upper = first_char.to_ascii_uppercase();

        for i in 0..=(h_bytes.len() - query_len) {
            let h_first = h_bytes[i];
            if (h_first == first_lower || h_first == first_upper)
                && h_bytes[i + 1..i + query_len].eq_ignore_ascii_case(&query_bytes[1..])
            {
                return true;
            }
        }
        false
    } else if let Some(query_lower) = query_lower_fallback {
        haystack.to_lowercase().contains(query_lower)
    } else {
        haystack.to_lowercase().contains(&query.to_lowercase())
    }
}

pub fn fuzzy_match_ignore_case(haystack: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let mut query_chars = query.chars().peekable();
    for h_char in haystack.chars() {
        if let Some(&q_char) = query_chars.peek() {
            // ⚡ Bolt Optimization: Compare iterators directly with `.eq()`
            // to avoid O(N) `.to_string()` heap allocations per character match.
            if h_char.to_lowercase().eq(q_char.to_lowercase()) {
                query_chars.next();
            }
        } else {
            return true;
        }
    }
    query_chars.peek().is_none()
}

pub fn redact_url(url: &Url) -> String {
    let mut redacted = url.clone();
    let pairs: Vec<(String, String)> = redacted
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    redacted.set_query(None);
    for (k, mut v) in pairs {
        if k == "code" || k == "state" {
            v = "[REDACTED]".to_string();
        }
        redacted.query_pairs_mut().append_pair(&k, &v);
    }
    redacted.to_string()
}

pub trait ApplyVectorDiffExt<T> {
    fn apply_diff(&mut self, diff: eyeball_im::VectorDiff<T>);
}

pub trait VectorOperations<T> {
    fn v_len(&self) -> usize;
    fn v_insert(&mut self, index: usize, value: T);
    fn v_remove(&mut self, index: usize);
    fn v_set(&mut self, index: usize, value: T);
    fn v_push_back(&mut self, value: T);
    fn v_push_front(&mut self, value: T);
    fn v_pop_back(&mut self);
    fn v_pop_front(&mut self);
    fn v_clear(&mut self);
    fn v_reset(&mut self, values: eyeball_im::Vector<T>);
    fn v_extend(&mut self, values: eyeball_im::Vector<T>);
    fn v_truncate(&mut self, length: usize);
}

impl<T: Clone> VectorOperations<T> for Vec<T> {
    fn v_len(&self) -> usize {
        self.len()
    }
    fn v_insert(&mut self, index: usize, value: T) {
        self.insert(index, value);
    }
    fn v_remove(&mut self, index: usize) {
        self.remove(index);
    }
    fn v_set(&mut self, index: usize, value: T) {
        self[index] = value;
    }
    fn v_push_back(&mut self, value: T) {
        self.push(value);
    }
    fn v_push_front(&mut self, value: T) {
        self.insert(0, value);
    }
    fn v_pop_back(&mut self) {
        self.pop();
    }
    fn v_pop_front(&mut self) {
        if !self.is_empty() {
            self.remove(0);
        }
    }
    fn v_clear(&mut self) {
        self.clear();
    }
    fn v_reset(&mut self, values: eyeball_im::Vector<T>) {
        *self = values.into_iter().collect();
    }
    fn v_extend(&mut self, values: eyeball_im::Vector<T>) {
        self.extend(values);
    }
    fn v_truncate(&mut self, length: usize) {
        self.truncate(length);
    }
}

impl<T: Clone> VectorOperations<T> for eyeball_im::Vector<T> {
    fn v_len(&self) -> usize {
        self.len()
    }
    fn v_insert(&mut self, index: usize, value: T) {
        self.insert(index, value);
    }
    fn v_remove(&mut self, index: usize) {
        self.remove(index);
    }
    fn v_set(&mut self, index: usize, value: T) {
        self.set(index, value);
    }
    fn v_push_back(&mut self, value: T) {
        self.push_back(value);
    }
    fn v_push_front(&mut self, value: T) {
        self.push_front(value);
    }
    fn v_pop_back(&mut self) {
        self.pop_back();
    }
    fn v_pop_front(&mut self) {
        self.pop_front();
    }
    fn v_clear(&mut self) {
        self.clear();
    }
    fn v_reset(&mut self, values: eyeball_im::Vector<T>) {
        *self = values;
    }
    fn v_extend(&mut self, values: eyeball_im::Vector<T>) {
        self.extend(values);
    }
    fn v_truncate(&mut self, length: usize) {
        self.truncate(length);
    }
}

impl<C: VectorOperations<T>, T: Clone> ApplyVectorDiffExt<T> for C {
    fn apply_diff(&mut self, diff: eyeball_im::VectorDiff<T>) {
        match diff {
            eyeball_im::VectorDiff::Insert { index, value } => {
                if index <= self.v_len() {
                    self.v_insert(index, value);
                } else {
                    self.v_push_back(value);
                }
            }
            eyeball_im::VectorDiff::Remove { index } => {
                if index < self.v_len() {
                    self.v_remove(index);
                }
            }
            eyeball_im::VectorDiff::Set { index, value } => {
                if index < self.v_len() {
                    self.v_set(index, value);
                }
            }
            eyeball_im::VectorDiff::Reset { values } => {
                self.v_reset(values);
            }
            eyeball_im::VectorDiff::PushBack { value } => {
                self.v_push_back(value);
            }
            eyeball_im::VectorDiff::PushFront { value } => {
                self.v_push_front(value);
            }
            eyeball_im::VectorDiff::PopBack => {
                self.v_pop_back();
            }
            eyeball_im::VectorDiff::PopFront => {
                self.v_pop_front();
            }
            eyeball_im::VectorDiff::Clear => {
                self.v_clear();
            }
            eyeball_im::VectorDiff::Append { values } => {
                self.v_extend(values);
            }
            eyeball_im::VectorDiff::Truncate { length } => {
                self.v_truncate(length);
            }
        }
    }
}
