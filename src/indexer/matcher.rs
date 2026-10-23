use crate::indexer::model::AppItem;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

/// 匹配结果
#[derive(Debug, Clone)]
pub struct MatchedApp {
    pub item: AppItem,
    pub score: i64,
}

pub struct AppMatcher {
    matcher: SkimMatcherV2,
}

impl Default for AppMatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl AppMatcher {
    pub fn new() -> Self {
        Self {
            matcher: SkimMatcherV2::default(),
        }
    }

    /// 对应用程序列表进行智能评分加权与模糊排序
    pub fn match_apps<'a>(
        &self,
        apps: &'a [AppItem],
        query: &str,
        limit: usize,
    ) -> Vec<AppItem> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return apps.iter().take(limit).cloned().collect();
        }

        let mut matched: Vec<MatchedApp> = Vec::new();

        for app in apps {
            let name_lower = app.name.to_lowercase();
            let mut final_score: i64 = 0;

            // 1. 完全匹配最高优先
            if name_lower == q {
                final_score += 10_000;
            } else if name_lower.starts_with(&q) {
                // 2. 原名直接前缀匹配
                final_score += 5_000 + (100 - name_lower.len() as i64).max(0);
            }

            // 3. 拼音首字母简拼匹配 (如 "wx" 命中 "微信")
            if app.pinyin_abbr.starts_with(&q) {
                final_score += 4_000 + (50 - app.pinyin_abbr.len() as i64).max(0);
            } else if app.pinyin_abbr.contains(&q) {
                final_score += 2_000;
            }

            // 4. 全拼前缀匹配 (如 "weixin" 命中 "微信")
            if app.pinyin_full.starts_with(&q) {
                final_score += 3_500;
            } else if app.pinyin_full.contains(&q) {
                final_score += 1_500;
            }

            // 5. 模糊算法综合评分
            if let Some(fuzzy_score) = self.matcher.fuzzy_match(&app.name, &q) {
                final_score += fuzzy_score;
            } else if let Some(fuzzy_abbr) = self.matcher.fuzzy_match(&app.pinyin_abbr, &q) {
                final_score += fuzzy_abbr * 2;
            }

            if final_score > 0 {
                matched.push(MatchedApp {
                    item: app.clone(),
                    score: final_score,
                });
            }
        }

        // 按得分从高到低降序排序
        matched.sort_by(|a, b| b.score.cmp(&a.score));

        matched.into_iter().take(limit).map(|m| m.item).collect()
    }
}
