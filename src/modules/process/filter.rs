use super::models::ProcessItem;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

pub struct ProcessFilter {
    matcher: SkimMatcherV2,
}

impl ProcessFilter {
    pub fn new() -> Self {
        Self {
            matcher: SkimMatcherV2::default(),
        }
    }

    /// Filtra e classifica os processos com base na consulta fuzzy (pelo nome ou PID)
    pub fn filter<'a>(&self, query: &str, items: &'a [ProcessItem]) -> Vec<&'a ProcessItem> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return items.iter().collect();
        }

        let mut scored: Vec<(&'a ProcessItem, i64)> = items
            .iter()
            .filter_map(|item| {
                let name_score = self.matcher.fuzzy_match(&item.name, trimmed);
                let pid_score = self
                    .matcher
                    .fuzzy_match(&item.pid.to_string(), trimmed);

                let best_score = match (name_score, pid_score) {
                    (Some(s1), Some(s2)) => Some(s1.max(s2)),
                    (Some(s1), None) => Some(s1),
                    (None, Some(s2)) => Some(s2),
                    (None, None) => None,
                };

                best_score.map(|score| (item, score))
            })
            .collect();

        // Ordenar por maior pontuação fuzzy
        scored.sort_by(|a, b| b.1.cmp(&a.1));
        scored.into_iter().map(|(item, _)| item).collect()
    }
}
