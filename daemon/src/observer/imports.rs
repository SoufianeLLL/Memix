pub fn extract_imports(ext: &str, content: &str) -> Vec<String> {
	let mut out = Vec::new();
	for line in content.lines().take(2000) {
		let l = line.trim();
		if l.is_empty() {
			continue;
		}
		match ext {
			"ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => {
				if let Some(rest) = l.strip_prefix("import ") {
					if let Some(idx) = rest.rfind(" from ") {
						let mut s = rest[(idx + 6)..].trim().trim_end_matches(';').trim();
						s = s.trim_matches('"').trim_matches('\'');
						if !s.is_empty() {
							out.push(s.to_string());
						}
					}
				}
			}
			"rs" => {
				if let Some(rest) = l.strip_prefix("use ") {
					let p = rest.split(';').next().unwrap_or("").trim();
					if !p.is_empty() {
						out.push(p.to_string());
					}
				}
			}
			"py" => {
				if let Some(rest) = l.strip_prefix("import ") {
					let p = rest.split_whitespace().next().unwrap_or("");
					if !p.is_empty() {
						out.push(p.to_string());
					}
				} else if let Some(rest) = l.strip_prefix("from ") {
					let p = rest.split_whitespace().next().unwrap_or("");
					if !p.is_empty() {
						out.push(p.to_string());
					}
				}
			}
			_ => {}
		}
	}
	out
}

pub fn signature_head(body: &str) -> String {
	let first = body.lines().next().unwrap_or("").trim();
	first.split('{').next().unwrap_or(first).trim().to_string()
}
