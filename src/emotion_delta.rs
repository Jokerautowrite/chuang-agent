//! EmotionDeltaExtractor：对话 → 五轴情感修正量（可拔插）。
//!
//! 铁律：接口先行；先规则版（零模型成本、确定性强），后模型版（复用现有
//! provider 通道，只回几个 delta 值，不引入额外 fallback 复杂度）。
//! 规则版是默认接入；模型版是可插增强，测试用 ScriptedResponder 即可。

use crate::emotion_slot::EmotionDelta;
use crate::responder::{Responder, ResponderRequest};

/// 从一轮对话（主人输入 + 创回复）中提取五轴 delta。
pub trait EmotionDeltaExtractor {
    fn extract(&self, user_input: &str, assistant_reply: &str) -> EmotionDelta;
}

/// 规则版提取器：中文情感词表 + 感叹/长度启发式，完全确定性。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleEmotionDeltaExtractor;

impl Default for RuleEmotionDeltaExtractor {
    fn default() -> Self {
        Self
    }
}

/// 常用情绪词表（陪伴场景，去重排序）。
const POSITIVE_WORDS: &[&str] = &[
    "不错",
    "可爱",
    "厉害",
    "喜欢",
    "开心",
    "感动",
    "放心",
    "满意",
    "温暖",
    "爽",
    "甜",
    "真棒",
    "真好",
    "真行",
    "种草",
    "惊喜",
    "期待",
    "安心",
    "幸福",
    "幸运",
    "顺利",
    "成功",
    "爱",
    "棒",
    "牛",
    "赞",
    "靠谱",
    "舒服",
    "高兴",
    "好开心",
];

const NEGATIVE_WORDS: &[&str] = &[
    "emo", "无语", "压力", "哭", "失眠", "孤独", "委屈", "崩溃", "失望", "孤单", "寂寞", "害怕",
    "难受", "心痛", "心累", "恐惧", "想哭", "恨", "悲观", "痛苦", "烦", "累", "糟心", "紧张",
    "焦虑", "生气", "沮丧", "郁闷", "讨厌", "伤心", "难过", "过分", "气死", "可恶", "糟糕",
];

const INTENSE_WORDS: &[&str] = &[
    "救命", "太", "彻底", "急", "抓狂", "炸", "激动", "爆炸", "疯", "疯狂", "兴奋", "非常", "极其",
    "特别", "超级", "巨", "愤怒", "气死", "火大", "咆哮",
];

const PRAISE_WORDS: &[&str] = &[
    "贴心", "懂我", "懂你", "佩服", "崇拜", "爱你", "真好", "谢谢", "感谢", "靠谱", "乖", "棒",
    "厉害", "强", "牛", "聪明", "优秀", "好棒", "真棒",
];

const CRITICISM_WORDS: &[&str] = &["差劲", "不行", "没用", "废物", "你错了", "讨厌你", "烦你"];

const INTIMACY_WORDS: &[&str] = &[
    "想你",
    "想你了",
    "好想你",
    "抱抱",
    "抱",
    "亲",
    "贴贴",
    "陪我",
    "聊聊",
    "心里话",
    "秘密",
    "倾诉",
    "说说",
    "晚安",
    "早安",
    "睡了吗",
    "在吗",
];

fn count_matches(text: &str, words: &[&str]) -> usize {
    let lower = text.to_lowercase();
    words.iter().filter(|word| lower.contains(**word)).count()
}

impl EmotionDeltaExtractor for RuleEmotionDeltaExtractor {
    fn extract(&self, user_input: &str, assistant_reply: &str) -> EmotionDelta {
        let text = format!("{user_input} {assistant_reply}");
        let user_only = user_input;
        let user_chars = user_input.chars().count();
        let total_chars = text.chars().count();

        let positive = count_matches(&text, POSITIVE_WORDS);
        let negative = count_matches(&text, NEGATIVE_WORDS);
        let intense = count_matches(&text, INTENSE_WORDS);
        let praise = count_matches(&user_only, PRAISE_WORDS);
        let criticism = count_matches(&user_only, CRITICISM_WORDS);
        let intimacy = count_matches(&user_only, INTIMACY_WORDS);
        let exclamations = user_input.matches(['！', '!']).count();

        // 愉悦度：正向词 +0.12，负向词 -0.14，封顶 ±0.6。
        let valence = ((positive as f64) * 0.12 - (negative as f64) * 0.14).clamp(-0.6, 0.6);

        // 唤醒度：强度词 +0.15、感叹号 +0.04；负向但无强度词 → 低落（负唤醒）。
        let arousal = if intense == 0 && negative > 0 {
            (-0.12 * negative as f64).clamp(-0.4, 0.0)
        } else {
            (intense as f64 * 0.15 + exclamations as f64 * 0.04).clamp(0.0, 0.6)
        };

        // 骄傲：被夸奖 +0.15，被批评 -0.12。
        let pride = (praise as f64 * 0.15 - criticism as f64 * 0.12).clamp(-0.4, 0.5);

        // 连接需求：主人主动倾诉/亲昵 → 需求下降（接近感上升）。
        let connection =
            (-0.08 * intimacy as f64 - 0.05 * (negative > 0) as u8 as f64).clamp(-0.3, 0.0);

        // 沉浸度：对话越长、情绪密度越高 → 越沉浸。
        let length_factor = (total_chars as f64 / 1000.0 * 0.05).min(0.2);
        let density_factor = ((positive + negative + intense) as f64 / 8.0 * 0.1).min(0.2);
        let immersion = (length_factor + density_factor).min(0.4);

        let _ = user_chars; // 长度已由 total_chars 覆盖
        EmotionDelta {
            connection: Some(connection),
            pride: Some(pride),
            valence: Some(valence),
            arousal: Some(arousal),
            immersion: Some(immersion),
        }
    }
}

/// 模型版提取器：复用任意 Responder（如现有 provider 通道），
/// 只让模型回五个 delta 值。失败返回全 None（不阻断主流程）。
#[derive(Debug, Clone)]
pub struct ModelEmotionDeltaExtractor<R: Responder> {
    responder: R,
}

impl<R: Responder> ModelEmotionDeltaExtractor<R> {
    pub fn new(responder: R) -> Self {
        Self { responder }
    }
}

/// 生成模型提取 prompt：要求只回 JSON 五轴 delta。
pub fn build_delta_prompt(user_input: &str, assistant_reply: &str) -> String {
    format!(
        "你是情感陪伴系统的情绪分析器。根据下面这轮对话，判断主人情绪的连续变化，\
         只输出一个 JSON 对象，不要任何解释或 markdown 代码块。\n\
         字段（范围）：connection 连接需求 0..1；pride 骄傲 -1..1；valence 愉悦度 -1..1；\
         arousal 唤醒度 -1..1；immersion 沉浸度 0..1。没有变化的轴写 null。\n\n\
         主人：{user_input}\n\
         创：{assistant_reply}\n\n\
         JSON：{{\"connection\":null,\"pride\":0.0,\"valence\":0.0,\"arousal\":0.0,\"immersion\":0.0}}"
    )
}

/// 宽容解析模型输出的 JSON delta（容忍 ```json 围栏、前后缀文本）。
pub fn parse_delta_json(body: &str) -> EmotionDelta {
    let Some(start) = body.find('{') else {
        return EmotionDelta::default();
    };
    let Some(end) = body.rfind('}') else {
        return EmotionDelta::default();
    };
    if end <= start {
        return EmotionDelta::default();
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&body[start..=end]) else {
        return EmotionDelta::default();
    };
    let Some(obj) = value.as_object() else {
        return EmotionDelta::default();
    };

    let num = |key: &str| -> Option<f64> {
        obj.get(key)
            .and_then(|v| v.as_f64())
            .map(|v| v.clamp(-1.0, 1.0))
    };

    EmotionDelta {
        connection: num("connection").map(|v| v.clamp(0.0, 1.0)),
        pride: num("pride"),
        valence: num("valence"),
        arousal: num("arousal"),
        immersion: num("immersion").map(|v| v.clamp(0.0, 1.0)),
    }
}

impl<R: Responder> EmotionDeltaExtractor for ModelEmotionDeltaExtractor<R> {
    fn extract(&self, user_input: &str, assistant_reply: &str) -> EmotionDelta {
        let prompt = build_delta_prompt(user_input, assistant_reply);
        let output = self.responder.generate(&ResponderRequest {
            prompt,
            user_input: user_input.to_string(),
            recall_hit_count: 0,
        });
        parse_delta_json(&output.body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::responder::ScriptedResponder;

    fn extract(user: &str, reply: &str) -> EmotionDelta {
        RuleEmotionDeltaExtractor::default().extract(user, reply)
    }

    #[test]
    fn neutral_conversation_stays_flat() {
        let delta = extract("帮我看看这个文件", "好的，我看了。");
        assert!(delta.valence.unwrap().abs() < 0.15);
        assert!(delta.arousal.unwrap().abs() < 0.15);
        assert!(delta.pride.unwrap().abs() < 0.15);
    }

    #[test]
    fn positive_message_raises_valence() {
        let delta = extract("今天好开心，项目终于成功了！", "太好了，为你高兴！");
        assert!(delta.valence.unwrap() > 0.2);
        assert!(delta.immersion.unwrap() > 0.0);
    }

    #[test]
    fn angry_message_lowers_valence_and_raises_arousal() {
        let delta = extract("气死我了！他太过分了！！", "别急，慢慢说。");
        assert!(delta.valence.unwrap() < -0.2);
        assert!(delta.arousal.unwrap() > 0.1);
    }

    #[test]
    fn sad_without_intensity_drops_arousal() {
        let delta = extract("最近有点难过，觉得孤独。", "我在呢。");
        assert!(delta.valence.unwrap() < -0.2);
        assert!(delta.arousal.unwrap() < 0.0);
        assert!(delta.connection.unwrap() < 0.0);
    }

    #[test]
    fn praise_raises_pride() {
        let delta = extract("你真厉害，帮了我大忙，谢谢你！", "不客气！");
        assert!(delta.pride.unwrap() > 0.2);
        assert!(delta.valence.unwrap() > 0.0);
    }

    #[test]
    fn criticism_lowers_pride() {
        let delta = extract("你不行，这都做不好。", "抱歉。");
        assert!(delta.pride.unwrap() < 0.0);
    }

    #[test]
    fn intimacy_lowers_connection_demand() {
        let delta = extract("好想你，抱抱我", "抱抱你。");
        assert!(delta.connection.unwrap() < -0.1);
    }

    #[test]
    fn parse_delta_json_accepts_plain_and_fenced() {
        let plain = parse_delta_json(
            r#"{"connection":0.1,"pride":0.2,"valence":0.3,"arousal":0.4,"immersion":0.5}"#,
        );
        assert_eq!(plain.connection, Some(0.1));
        assert_eq!(plain.pride, Some(0.2));
        assert_eq!(plain.valence, Some(0.3));
        assert_eq!(plain.arousal, Some(0.4));
        assert_eq!(plain.immersion, Some(0.5));

        let fenced = parse_delta_json(
            "好的\n```json\n{\"valence\":-0.2,\"arousal\":0.1,\"pride\":null}\n```",
        );
        assert_eq!(fenced.valence, Some(-0.2));
        assert_eq!(fenced.arousal, Some(0.1));
        assert_eq!(fenced.pride, None);
    }

    #[test]
    fn parse_delta_json_clamps_ranges() {
        let delta = parse_delta_json(
            r#"{"connection":5.0,"pride":-5.0,"valence":2.0,"arousal":-2.0,"immersion":9.0}"#,
        );
        assert_eq!(delta.connection, Some(1.0));
        assert_eq!(delta.pride, Some(-1.0));
        assert_eq!(delta.valence, Some(1.0));
        assert_eq!(delta.arousal, Some(-1.0));
        assert_eq!(delta.immersion, Some(1.0));
    }

    #[test]
    fn parse_delta_json_malformed_returns_default() {
        assert_eq!(parse_delta_json("not json at all"), EmotionDelta::default());
        assert_eq!(parse_delta_json("[]"), EmotionDelta::default());
        assert_eq!(parse_delta_json(""), EmotionDelta::default());
    }

    #[test]
    fn model_extractor_uses_responder_and_parses() {
        let scripted = ScriptedResponder::new(
            "deepseek-v4-flash",
            r#"{"connection":null,"pride":0.3,"valence":0.4,"arousal":0.1,"immersion":0.2}"#,
        );
        let extractor = ModelEmotionDeltaExtractor::new(scripted);
        let delta = extractor.extract("你真棒", "谢谢你！");
        assert_eq!(delta.pride, Some(0.3));
        assert_eq!(delta.valence, Some(0.4));
        assert_eq!(delta.connection, None);
    }

    #[test]
    fn model_extractor_falls_back_to_empty_on_bad_body() {
        let scripted = ScriptedResponder::new("deepseek-v4-flash", "抱歉，我无法分析。");
        let extractor = ModelEmotionDeltaExtractor::new(scripted);
        let delta = extractor.extract("你好", "你好");
        assert_eq!(delta, EmotionDelta::default());
    }
}
