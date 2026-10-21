//! 任务目标解析：从 `S.ChangeQuest.task_list` 的「前往 地图名(mapfile)」行提取导航目标。
//!
//! 服务端契约（第 26 轮）：`task_list` 里会有一行
//! `前往 <地图名>(<mapfile>)`，例如 `前往 奥玛洞穴(OmaCavern)`；无目标的任务写
//! `该任务无导航目标`。解析**必须容错**：格式不符返回 `None`，不 panic；
//! 导航用的门格来自 `S.NewMapInfo.movements`，本模块不读任何配置文件。

/// 任务追踪文本里解析出的导航目标。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestTarget {
    /// 地图显示名（服务端文本原样，例如 `奥玛洞穴`）。
    pub map_name: String,
    /// 地图文件名（例如 `OmaCavern`）。
    pub map_file: String,
}

/// 从任务进度文本行里解析导航目标；没有/格式不符返回 `None`。
///
/// 接受：
/// * `前往 奥玛洞穴(OmaCavern)`；
/// * `前往 奥玛洞穴（OmaCavern）`（全角括号容错）；
/// * 前后空白、行内多余空格。
///
/// 拒绝：
/// * `该任务无导航目标`、`去某地`、空行；
/// * 空地图名/空 mapfile、括号不闭合。
#[must_use]
pub fn parse_task_list(lines: &[String]) -> Option<QuestTarget> {
    lines.iter().find_map(|line| parse_line(line))
}

/// 解析单行；供测试与逐行诊断用。
#[must_use]
pub fn parse_line(line: &str) -> Option<QuestTarget> {
    let line = line.trim();
    let rest = line
        .strip_prefix("前往")
        .or_else(|| line.strip_prefix("前往："))
        .or_else(|| line.strip_prefix("前往:"))?
        .trim();
    let (map_name, map_file) = if let Some(open) = rest.rfind('（') {
        let close = rest.rfind('）')?;
        if close <= open + '（'.len_utf8() || close + '）'.len_utf8() != rest.len() {
            return None;
        }
        (
            rest[..open].trim(),
            rest[open + '（'.len_utf8()..close].trim(),
        )
    } else {
        let open = rest.rfind('(')?;
        let close = rest.rfind(')')?;
        if close <= open + 1 || close + 1 != rest.len() {
            return None;
        }
        (rest[..open].trim(), rest[open + 1..close].trim())
    };
    if map_name.is_empty() || map_file.is_empty() || map_file.chars().any(char::is_whitespace) {
        return None;
    }
    Some(QuestTarget {
        map_name: map_name.to_owned(),
        map_file: map_file.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_go_to_target() {
        let lines = vec!["前往 奥玛洞穴(OmaCavern)".to_owned()];
        assert_eq!(
            parse_task_list(&lines),
            Some(QuestTarget {
                map_name: "奥玛洞穴".to_owned(),
                map_file: "OmaCavern".to_owned(),
            })
        );
        assert_eq!(
            parse_line("  前往 奥玛洞穴（OmaCavern）  "),
            Some(QuestTarget {
                map_name: "奥玛洞穴".to_owned(),
                map_file: "OmaCavern".to_owned(),
            })
        );
    }

    #[test]
    fn malformed_and_no_target_are_ignored_without_panic() {
        for line in [
            "该任务无导航目标",
            "去某地",
            "",
            "前往",
            "前往 奥玛洞穴",
            "前往 (OmaCavern)",
            "前往 奥玛洞穴()",
            "前往 奥玛洞穴(OmaCavern",
            "前往 奥玛洞穴（OmaCavern)",
            "前往 奥玛 洞穴(Oma Cavern)",
        ] {
            assert_eq!(parse_line(line), None, "不应误解析：{line:?}");
        }
        assert_eq!(parse_task_list(&[]), None);
        assert_eq!(parse_task_list(&["击杀 鹿 3/10".to_owned()]), None);
    }

    #[test]
    fn finds_target_after_progress_lines() {
        let lines = vec![
            "击杀 鹿 3/10".to_owned(),
            "该任务无导航目标".to_owned(),
            "前往 奥玛洞穴(OmaCavern)".to_owned(),
        ];
        assert_eq!(
            parse_task_list(&lines).map(|target| (target.map_name, target.map_file)),
            Some(("奥玛洞穴".to_owned(), "OmaCavern".to_owned()))
        );
    }
}
