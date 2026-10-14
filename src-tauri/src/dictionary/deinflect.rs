//! Rule-based deinflection of Japanese verbs and i-adjectives.
//!
//! Each [`Rule`] rewrites a suffix of an inflected string into the suffix of a less inflected one.
//! Strings are labelled with the categories (bit flags) they may belong to, which restricts how
//! rules chain: the original text may belong to any category, and every rule outputs exactly one.
//! The dictionary later checks that the final category matches the part of speech of the entry.

use std::collections::HashSet;
use std::sync::OnceLock;

pub const V1: u32 = 1 << 0;
pub const V5: u32 = 1 << 1;
pub const VS: u32 = 1 << 2;
pub const VK: u32 = 1 << 3;
pub const ADJ_I: u32 = 1 << 4;
/// Continuative (masu-stem) form: 食べ, 書き, し.
pub const STEM: u32 = 1 << 5;
/// て/で form.
pub const TE: u32 = 1 << 6;
/// た/だ form.
pub const TA: u32 = 1 << 7;
/// Polite ます form.
pub const MASU: u32 = 1 << 8;
pub const ANY: u32 = V1 | V5 | VS | VK | ADJ_I | STEM | TE | TA | MASU;

/// Bounds the chain length so malformed input cannot explode the search.
const MAX_REASONS: usize = 10;

#[derive(Debug, Clone, Copy)]
struct Rule {
    kana_in: &'static str,
    kana_out: &'static str,
    rules_in: u32,
    rules_out: u32,
    reasons: &'static [&'static str],
}

/// A possible less inflected form of a text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub term: String,
    /// Category of `term`; for the untouched text this is [`ANY`].
    pub rules: u32,
    /// Inflections found, outermost first (e.g. `past`, `negative`, `passive`).
    pub reasons: Vec<&'static str>,
}

struct GodanRow {
    dictionary: &'static str,
    i: &'static str,
    a: &'static str,
    e: &'static str,
    o: &'static str,
    past: &'static str,
    te: &'static str,
}

const GODAN_ROWS: [GodanRow; 9] = [
    GodanRow {
        dictionary: "う",
        i: "い",
        a: "わ",
        e: "え",
        o: "お",
        past: "った",
        te: "って",
    },
    GodanRow {
        dictionary: "く",
        i: "き",
        a: "か",
        e: "け",
        o: "こ",
        past: "いた",
        te: "いて",
    },
    GodanRow {
        dictionary: "ぐ",
        i: "ぎ",
        a: "が",
        e: "げ",
        o: "ご",
        past: "いだ",
        te: "いで",
    },
    GodanRow {
        dictionary: "す",
        i: "し",
        a: "さ",
        e: "せ",
        o: "そ",
        past: "した",
        te: "して",
    },
    GodanRow {
        dictionary: "つ",
        i: "ち",
        a: "た",
        e: "て",
        o: "と",
        past: "った",
        te: "って",
    },
    GodanRow {
        dictionary: "ぬ",
        i: "に",
        a: "な",
        e: "ね",
        o: "の",
        past: "んだ",
        te: "んで",
    },
    GodanRow {
        dictionary: "ぶ",
        i: "び",
        a: "ば",
        e: "べ",
        o: "ぼ",
        past: "んだ",
        te: "んで",
    },
    GodanRow {
        dictionary: "む",
        i: "み",
        a: "ま",
        e: "め",
        o: "も",
        past: "んだ",
        te: "んで",
    },
    GodanRow {
        dictionary: "る",
        i: "り",
        a: "ら",
        e: "れ",
        o: "ろ",
        past: "った",
        te: "って",
    },
];

/// Auxiliary verbs attached to a te-form: (suffix after て, dictionary form category, reason).
const TE_AUXILIARIES: [(&str, u32, &str); 8] = [
    ("いる", V1, "progressive"),
    ("しまう", V5, "completive"),
    ("おく", V5, "preparatory"),
    ("ある", V5, "resultative"),
    ("いく", V5, "continuing"),
    ("みる", V1, "trial"),
    ("あげる", V1, "benefactive"),
    ("くれる", V1, "benefactive"),
];

fn rules() -> &'static [Rule] {
    static RULES: OnceLock<Vec<Rule>> = OnceLock::new();
    RULES.get_or_init(build_rules)
}

fn build_rules() -> Vec<Rule> {
    let mut rules = Vec::new();
    add_polite_rules(&mut rules);
    add_stem_suffix_rules(&mut rules);
    add_adjective_rules(&mut rules);
    add_ichidan_rules(&mut rules);
    add_godan_rules(&mut rules);
    add_suru_rules(&mut rules);
    add_kuru_rules(&mut rules);
    add_te_auxiliary_rules(&mut rules);
    add_conditional_rules(&mut rules);
    rules
}

fn rule(
    kana_in: &'static str,
    kana_out: &'static str,
    rules_in: u32,
    rules_out: u32,
    reasons: &'static [&'static str],
) -> Rule {
    Rule {
        kana_in,
        kana_out,
        rules_in,
        rules_out,
        reasons,
    }
}

/// Leaks a formatted string so it can live in a `'static` rule table (built once per process).
fn leak(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}

fn add_polite_rules(rules: &mut Vec<Rule>) {
    rules.push(rule("ました", "ます", ANY, MASU, &["past"]));
    rules.push(rule("ません", "ます", ANY, MASU, &["negative"]));
    rules.push(rule(
        "ませんでした",
        "ます",
        ANY,
        MASU,
        &["negative", "past"],
    ));
    rules.push(rule("ましょう", "ます", ANY, MASU, &["volitional"]));
    rules.push(rule("まして", "ます", ANY, MASU, &["te-form"]));
    rules.push(rule("ます", "", MASU, STEM, &["polite"]));
}

fn add_stem_suffix_rules(rules: &mut Vec<Rule>) {
    rules.push(rule("たい", "", ADJ_I, STEM, &["desire"]));
    rules.push(rule("たがる", "", V5, STEM, &["desire (third person)"]));
    rules.push(rule("ながら", "", ANY, STEM, &["while"]));
    rules.push(rule("つつ", "", ANY, STEM, &["while"]));
    rules.push(rule("そう", "", ANY, STEM, &["appearance"]));
    rules.push(rule("すぎる", "", V1, STEM, &["excess"]));
    rules.push(rule("やすい", "", ADJ_I, STEM, &["easy to"]));
    rules.push(rule("にくい", "", ADJ_I, STEM, &["hard to"]));
    rules.push(rule("なさい", "", ANY, STEM, &["polite imperative"]));
}

fn add_adjective_rules(rules: &mut Vec<Rule>) {
    rules.push(rule("くない", "い", ADJ_I, ADJ_I, &["negative"]));
    rules.push(rule("かった", "い", ADJ_I, ADJ_I, &["past"]));
    rules.push(rule("くて", "い", ADJ_I, ADJ_I, &["te-form"]));
    rules.push(rule("く", "い", ADJ_I, ADJ_I, &["adverbial"]));
    rules.push(rule("ければ", "い", ADJ_I, ADJ_I, &["conditional (ba)"]));
    rules.push(rule(
        "かったら",
        "い",
        ADJ_I,
        ADJ_I,
        &["conditional (tara)"],
    ));
    rules.push(rule("さ", "い", ANY, ADJ_I, &["nominalization"]));
    rules.push(rule("そう", "い", ANY, ADJ_I, &["appearance"]));
    rules.push(rule("すぎる", "い", V1 | ADJ_I, ADJ_I, &["excess"]));
    rules.push(rule("ないで", "ない", ANY, ADJ_I, &["without doing"]));
}

fn add_ichidan_rules(rules: &mut Vec<Rule>) {
    rules.push(rule("", "る", STEM, V1, &[]));
    rules.push(rule("ない", "る", ADJ_I, V1, &["negative"]));
    rules.push(rule("ず", "る", ANY, V1, &["negative (zu)"]));
    rules.push(rule("ぬ", "る", ANY, V1, &["negative (nu)"]));
    rules.push(rule("た", "る", TA, V1, &["past"]));
    rules.push(rule("て", "る", TE, V1, &["te-form"]));
    rules.push(rule("られる", "る", V1, V1, &["potential or passive"]));
    rules.push(rule("れる", "る", V1, V1, &["potential (colloquial)"]));
    rules.push(rule("させる", "る", V1, V1, &["causative"]));
    rules.push(rule("よう", "る", ANY, V1, &["volitional"]));
    rules.push(rule("ろ", "る", ANY, V1, &["imperative"]));
    rules.push(rule("よ", "る", ANY, V1, &["imperative"]));
    rules.push(rule("れば", "る", ANY, V1, &["conditional (ba)"]));
}

fn add_godan_rules(rules: &mut Vec<Rule>) {
    for row in &GODAN_ROWS {
        add_godan_row_rules(rules, row);
    }
    // 行く and its kana spelling take った/って like the う/つ/る verbs.
    rules.push(rule("行った", "行く", TA, V5, &["past"]));
    rules.push(rule("行って", "行く", TE, V5, &["te-form"]));
    rules.push(rule("いった", "いく", TA, V5, &["past"]));
    rules.push(rule("いって", "いく", TE, V5, &["te-form"]));
    // Honorific verbs such as いらっしゃる take い in the polite stem.
    rules.push(rule("い", "る", STEM, V5, &[]));
}

fn add_godan_row_rules(rules: &mut Vec<Rule>, row: &GodanRow) {
    let dictionary = row.dictionary;
    rules.push(rule(row.i, dictionary, STEM, V5, &[]));
    rules.push(rule(
        leak(format!("{}ない", row.a)),
        dictionary,
        ADJ_I,
        V5,
        &["negative"],
    ));
    rules.push(rule(
        leak(format!("{}ず", row.a)),
        dictionary,
        ANY,
        V5,
        &["negative (zu)"],
    ));
    rules.push(rule(
        leak(format!("{}ぬ", row.a)),
        dictionary,
        ANY,
        V5,
        &["negative (nu)"],
    ));
    rules.push(rule(row.past, dictionary, TA, V5, &["past"]));
    rules.push(rule(row.te, dictionary, TE, V5, &["te-form"]));
    rules.push(rule(
        leak(format!("{}る", row.e)),
        dictionary,
        V1,
        V5,
        &["potential"],
    ));
    rules.push(rule(
        leak(format!("{}れる", row.a)),
        dictionary,
        V1,
        V5,
        &["passive"],
    ));
    rules.push(rule(
        leak(format!("{}せる", row.a)),
        dictionary,
        V1,
        V5,
        &["causative"],
    ));
    rules.push(rule(
        leak(format!("{}される", row.a)),
        dictionary,
        V1,
        V5,
        &["causative passive"],
    ));
    rules.push(rule(
        leak(format!("{}う", row.o)),
        dictionary,
        ANY,
        V5,
        &["volitional"],
    ));
    rules.push(rule(row.e, dictionary, ANY, V5, &["imperative"]));
    rules.push(rule(
        leak(format!("{}ば", row.e)),
        dictionary,
        ANY,
        V5,
        &["conditional (ba)"],
    ));
}

fn add_suru_rules(rules: &mut Vec<Rule>) {
    rules.push(rule("し", "する", STEM, VS, &[]));
    rules.push(rule("しない", "する", ADJ_I, VS, &["negative"]));
    rules.push(rule("した", "する", TA, VS, &["past"]));
    rules.push(rule("して", "する", TE, VS, &["te-form"]));
    rules.push(rule("しよう", "する", ANY, VS, &["volitional"]));
    rules.push(rule("しろ", "する", ANY, VS, &["imperative"]));
    rules.push(rule("せよ", "する", ANY, VS, &["imperative"]));
    rules.push(rule("すれば", "する", ANY, VS, &["conditional (ba)"]));
    rules.push(rule("できる", "する", V1, VS, &["potential"]));
    rules.push(rule("される", "する", V1, VS, &["passive"]));
    rules.push(rule("させる", "する", V1, VS, &["causative"]));
}

fn add_kuru_rules(rules: &mut Vec<Rule>) {
    for (spelling, stem, negative) in [("来る", "来", "来"), ("くる", "き", "こ")] {
        let dictionary = spelling;
        rules.push(rule(stem, dictionary, STEM, VK, &[]));
        rules.push(rule(
            leak(format!("{negative}ない")),
            dictionary,
            ADJ_I,
            VK,
            &["negative"],
        ));
        rules.push(rule(
            leak(format!("{stem}た")),
            dictionary,
            TA,
            VK,
            &["past"],
        ));
        rules.push(rule(
            leak(format!("{stem}て")),
            dictionary,
            TE,
            VK,
            &["te-form"],
        ));
        rules.push(rule(
            leak(format!("{negative}よう")),
            dictionary,
            ANY,
            VK,
            &["volitional"],
        ));
        rules.push(rule(
            leak(format!("{negative}い")),
            dictionary,
            ANY,
            VK,
            &["imperative"],
        ));
        rules.push(rule(
            leak(format!("{negative}れば")),
            dictionary,
            ANY,
            VK,
            &["conditional (ba)"],
        ));
        rules.push(rule(
            leak(format!("{negative}られる")),
            dictionary,
            V1,
            VK,
            &["potential or passive"],
        ));
        rules.push(rule(
            leak(format!("{negative}させる")),
            dictionary,
            V1,
            VK,
            &["causative"],
        ));
    }
}

fn add_te_auxiliary_rules(rules: &mut Vec<Rule>) {
    for (auxiliary, category, reason) in TE_AUXILIARIES {
        rules.push(rule(
            leak(format!("て{auxiliary}")),
            "て",
            category,
            TE,
            leak_reasons(reason),
        ));
        rules.push(rule(
            leak(format!("で{auxiliary}")),
            "で",
            category,
            TE,
            leak_reasons(reason),
        ));
    }
    rules.push(rule("てる", "て", V1, TE, &["progressive (colloquial)"]));
    rules.push(rule("でる", "で", V1, TE, &["progressive (colloquial)"]));
    rules.push(rule("ちゃう", "て", V5, TE, &["completive (colloquial)"]));
    rules.push(rule("じゃう", "で", V5, TE, &["completive (colloquial)"]));
    rules.push(rule("とく", "て", V5, TE, &["preparatory (colloquial)"]));
    rules.push(rule("どく", "で", V5, TE, &["preparatory (colloquial)"]));
    rules.push(rule("てくる", "て", VK, TE, &["approaching"]));
    rules.push(rule("できる", "で", VK, TE, &["approaching"]));
    rules.push(rule("てもらう", "て", V5, TE, &["benefactive"]));
    rules.push(rule("でもらう", "で", V5, TE, &["benefactive"]));
}

fn leak_reasons(reason: &'static str) -> &'static [&'static str] {
    Box::leak(vec![reason].into_boxed_slice())
}

fn add_conditional_rules(rules: &mut Vec<Rule>) {
    rules.push(rule("たら", "た", ANY, TA, &["conditional (tara)"]));
    rules.push(rule("だら", "だ", ANY, TA, &["conditional (tara)"]));
    rules.push(rule("たり", "た", ANY, TA, &["representative (tari)"]));
    rules.push(rule("だり", "だ", ANY, TA, &["representative (tari)"]));
}

/// Returns `text` itself followed by every less inflected form reachable through the rules,
/// fewest inflections first.
pub fn deinflect(text: &str) -> Vec<Candidate> {
    let mut candidates = vec![Candidate {
        term: text.to_string(),
        rules: ANY,
        reasons: Vec::new(),
    }];
    let mut seen: HashSet<(String, u32)> = HashSet::new();
    seen.insert((text.to_string(), ANY));

    let mut index = 0;
    while index < candidates.len() {
        let current = candidates[index].clone();
        index += 1;
        if current.reasons.len() >= MAX_REASONS {
            continue;
        }

        for rule in rules() {
            if current.rules & rule.rules_in == 0 || !current.term.ends_with(rule.kana_in) {
                continue;
            }
            let base = &current.term[..current.term.len() - rule.kana_in.len()];
            let term = format!("{base}{}", rule.kana_out);
            if term.is_empty() || !seen.insert((term.clone(), rule.rules_out)) {
                continue;
            }

            let mut reasons = current.reasons.clone();
            reasons.extend_from_slice(rule.reasons);
            candidates.push(Candidate {
                term,
                rules: rule.rules_out,
                reasons,
            });
        }
    }
    candidates
}

/// Category flags matching a JMdict part-of-speech tag (0 when it is not an inflecting class).
pub fn category_of_tag(tag: &str) -> u32 {
    match tag {
        "v1" | "v1-s" => V1,
        "vk" => VK,
        "vs" | "vs-i" | "vs-s" => VS,
        "adj-i" | "adj-ix" => ADJ_I,
        _ if tag.starts_with("v5") => V5,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find<'a>(candidates: &'a [Candidate], term: &str, rules: u32) -> Option<&'a Candidate> {
        candidates
            .iter()
            .find(|candidate| candidate.term == term && candidate.rules & rules != 0)
    }

    fn reasons_of(text: &str, term: &str, rules: u32) -> Option<Vec<&'static str>> {
        find(&deinflect(text), term, rules).map(|candidate| candidate.reasons.clone())
    }

    #[test]
    fn the_untouched_text_is_always_the_first_candidate() {
        let candidates = deinflect("猫");

        assert_eq!(
            candidates[0],
            Candidate {
                term: "猫".to_string(),
                rules: ANY,
                reasons: vec![],
            }
        );
    }

    #[test]
    fn text_without_inflection_suffixes_only_yields_itself_and_its_ichidan_stem_guess() {
        let candidates = deinflect("猫");

        assert_eq!(
            candidates
                .iter()
                .map(|candidate| (candidate.term.as_str(), candidate.rules))
                .collect::<Vec<_>>(),
            vec![("猫", ANY), ("猫る", V1)]
        );
    }

    #[test]
    fn deinflects_ichidan_verbs() {
        assert_eq!(reasons_of("食べた", "食べる", V1), Some(vec!["past"]));
        assert_eq!(reasons_of("食べて", "食べる", V1), Some(vec!["te-form"]));
        assert_eq!(reasons_of("食べない", "食べる", V1), Some(vec!["negative"]));
        assert_eq!(
            reasons_of("食べよう", "食べる", V1),
            Some(vec!["volitional"])
        );
        assert_eq!(
            reasons_of("食べれば", "食べる", V1),
            Some(vec!["conditional (ba)"])
        );
        assert_eq!(reasons_of("食べろ", "食べる", V1), Some(vec!["imperative"]));
        assert_eq!(
            reasons_of("食べられる", "食べる", V1),
            Some(vec!["potential or passive"])
        );
        assert_eq!(
            reasons_of("食べさせる", "食べる", V1),
            Some(vec!["causative"])
        );
    }

    #[test]
    fn deinflects_every_godan_row() {
        let cases = [
            ("買わない", "買う"),
            ("買った", "買う"),
            ("書かない", "書く"),
            ("書いた", "書く"),
            ("泳がない", "泳ぐ"),
            ("泳いだ", "泳ぐ"),
            ("話さない", "話す"),
            ("話した", "話す"),
            ("待たない", "待つ"),
            ("待った", "待つ"),
            ("死なない", "死ぬ"),
            ("死んだ", "死ぬ"),
            ("遊ばない", "遊ぶ"),
            ("遊んだ", "遊ぶ"),
            ("読まない", "読む"),
            ("読んだ", "読む"),
            ("帰らない", "帰る"),
            ("帰った", "帰る"),
        ];

        for (inflected, dictionary) in cases {
            assert!(
                find(&deinflect(inflected), dictionary, V5).is_some(),
                "{inflected} should deinflect to {dictionary}"
            );
        }
    }

    #[test]
    fn deinflects_godan_te_forms_potential_passive_causative_and_volitional() {
        assert_eq!(reasons_of("書いて", "書く", V5), Some(vec!["te-form"]));
        assert_eq!(reasons_of("読んで", "読む", V5), Some(vec!["te-form"]));
        assert_eq!(reasons_of("書ける", "書く", V5), Some(vec!["potential"]));
        assert_eq!(reasons_of("書かれる", "書く", V5), Some(vec!["passive"]));
        assert_eq!(reasons_of("書かせる", "書く", V5), Some(vec!["causative"]));
        assert_eq!(
            reasons_of("書かされる", "書く", V5),
            Some(vec!["causative passive"])
        );
        assert_eq!(reasons_of("書こう", "書く", V5), Some(vec!["volitional"]));
        assert_eq!(reasons_of("書け", "書く", V5), Some(vec!["imperative"]));
        assert_eq!(
            reasons_of("書けば", "書く", V5),
            Some(vec!["conditional (ba)"])
        );
    }

    #[test]
    fn deinflects_the_irregular_iku_past_and_te_forms() {
        assert_eq!(reasons_of("行った", "行く", V5), Some(vec!["past"]));
        assert_eq!(reasons_of("行って", "行く", V5), Some(vec!["te-form"]));
        assert_eq!(reasons_of("いった", "いく", V5), Some(vec!["past"]));
        assert_eq!(reasons_of("いって", "いく", V5), Some(vec!["te-form"]));
    }

    #[test]
    fn deinflects_polite_forms_through_the_masu_stem() {
        assert_eq!(reasons_of("食べます", "食べる", V1), Some(vec!["polite"]));
        assert_eq!(
            reasons_of("食べました", "食べる", V1),
            Some(vec!["past", "polite"])
        );
        assert_eq!(
            reasons_of("食べません", "食べる", V1),
            Some(vec!["negative", "polite"])
        );
        assert_eq!(
            reasons_of("食べませんでした", "食べる", V1),
            Some(vec!["negative", "past", "polite"])
        );
        assert_eq!(
            reasons_of("食べましょう", "食べる", V1),
            Some(vec!["volitional", "polite"])
        );
        assert_eq!(reasons_of("書きます", "書く", V5), Some(vec!["polite"]));
        assert_eq!(
            reasons_of("買いました", "買う", V5),
            Some(vec!["past", "polite"])
        );
        assert_eq!(
            reasons_of("いらっしゃいます", "いらっしゃる", V5),
            Some(vec!["polite"])
        );
    }

    #[test]
    fn deinflects_suru_and_kuru_verbs() {
        assert_eq!(reasons_of("した", "する", VS), Some(vec!["past"]));
        assert_eq!(reasons_of("して", "する", VS), Some(vec!["te-form"]));
        assert_eq!(reasons_of("しない", "する", VS), Some(vec!["negative"]));
        assert_eq!(reasons_of("します", "する", VS), Some(vec!["polite"]));
        assert_eq!(reasons_of("しよう", "する", VS), Some(vec!["volitional"]));
        assert_eq!(reasons_of("できる", "する", VS), Some(vec!["potential"]));
        assert_eq!(
            reasons_of("された", "する", VS),
            Some(vec!["past", "passive"])
        );
        assert_eq!(reasons_of("来た", "来る", VK), Some(vec!["past"]));
        assert_eq!(reasons_of("来ない", "来る", VK), Some(vec!["negative"]));
        assert_eq!(reasons_of("来ます", "来る", VK), Some(vec!["polite"]));
        assert_eq!(reasons_of("きた", "くる", VK), Some(vec!["past"]));
        assert_eq!(reasons_of("こない", "くる", VK), Some(vec!["negative"]));
        assert_eq!(reasons_of("きて", "くる", VK), Some(vec!["te-form"]));
    }

    #[test]
    fn deinflects_i_adjectives() {
        assert_eq!(
            reasons_of("高くない", "高い", ADJ_I),
            Some(vec!["negative"])
        );
        assert_eq!(reasons_of("高かった", "高い", ADJ_I), Some(vec!["past"]));
        assert_eq!(reasons_of("高くて", "高い", ADJ_I), Some(vec!["te-form"]));
        assert_eq!(reasons_of("高く", "高い", ADJ_I), Some(vec!["adverbial"]));
        assert_eq!(
            reasons_of("高ければ", "高い", ADJ_I),
            Some(vec!["conditional (ba)"])
        );
        assert_eq!(
            reasons_of("高さ", "高い", ADJ_I),
            Some(vec!["nominalization"])
        );
        assert_eq!(
            reasons_of("高そう", "高い", ADJ_I),
            Some(vec!["appearance"])
        );
        assert_eq!(
            reasons_of("高くなかった", "高い", ADJ_I),
            Some(vec!["past", "negative"])
        );
    }

    #[test]
    fn chains_stacked_auxiliaries_outermost_first() {
        assert_eq!(
            reasons_of("食べさせられなかった", "食べる", V1),
            Some(vec![
                "past",
                "negative",
                "potential or passive",
                "causative"
            ])
        );
        assert_eq!(
            reasons_of("食べたくなかった", "食べる", V1),
            Some(vec!["past", "negative", "desire"])
        );
        assert_eq!(
            reasons_of("読んでいませんでした", "読む", V5),
            Some(vec!["negative", "past", "polite", "progressive", "te-form"])
        );
    }

    #[test]
    fn deinflects_te_auxiliaries() {
        assert_eq!(
            reasons_of("食べている", "食べる", V1),
            Some(vec!["progressive", "te-form"])
        );
        assert_eq!(
            reasons_of("食べてる", "食べる", V1),
            Some(vec!["progressive (colloquial)", "te-form"])
        );
        assert_eq!(
            reasons_of("読んでしまった", "読む", V5),
            Some(vec!["past", "completive", "te-form"])
        );
        assert_eq!(
            reasons_of("買っちゃった", "買う", V5),
            Some(vec!["past", "completive (colloquial)", "te-form"])
        );
        assert_eq!(
            reasons_of("見ておく", "見る", V1),
            Some(vec!["preparatory", "te-form"])
        );
    }

    #[test]
    fn deinflects_conditional_and_representative_forms() {
        assert_eq!(
            reasons_of("食べたら", "食べる", V1),
            Some(vec!["conditional (tara)", "past"])
        );
        assert_eq!(
            reasons_of("読んだり", "読む", V5),
            Some(vec!["representative (tari)", "past"])
        );
    }

    #[test]
    fn deinflects_stem_suffixes() {
        assert_eq!(reasons_of("食べながら", "食べる", V1), Some(vec!["while"]));
        assert_eq!(reasons_of("食べすぎる", "食べる", V1), Some(vec!["excess"]));
        assert_eq!(
            reasons_of("食べなさい", "食べる", V1),
            Some(vec!["polite imperative"])
        );
        assert_eq!(reasons_of("読みやすい", "読む", V5), Some(vec!["easy to"]));
        assert_eq!(
            reasons_of("読みにくかった", "読む", V5),
            Some(vec!["past", "hard to"])
        );
    }

    #[test]
    fn never_produces_an_empty_term() {
        for text in ["ます", "た", "て", "い", "し", "ない", "る"] {
            assert!(
                deinflect(text)
                    .iter()
                    .all(|candidate| !candidate.term.is_empty()),
                "{text} produced an empty candidate"
            );
        }
    }

    #[test]
    fn terminates_on_long_garbage_without_duplicates() {
        let candidates = deinflect("ませんでしたませんでしたませんでしたさせられたくなかった");
        let mut keys: Vec<(String, u32)> = candidates
            .iter()
            .map(|candidate| (candidate.term.clone(), candidate.rules))
            .collect();
        let total = keys.len();
        keys.sort();
        keys.dedup();

        assert_eq!(keys.len(), total);
        assert!(candidates
            .iter()
            .all(|candidate| candidate.reasons.len() <= MAX_REASONS + 5));
    }

    #[test]
    fn a_rule_is_not_applied_when_the_category_does_not_allow_it() {
        // 食べたい is an i-adjective form, so たい must not be treated as a te-form ending.
        let candidates = deinflect("食べたい");

        assert!(find(&candidates, "食べる", V1).is_some());
        assert!(find(&candidates, "食べた", TA).is_none());
    }

    #[test]
    fn maps_jmdict_tags_to_categories() {
        assert_eq!(category_of_tag("v1"), V1);
        assert_eq!(category_of_tag("v1-s"), V1);
        assert_eq!(category_of_tag("v5k"), V5);
        assert_eq!(category_of_tag("v5k-s"), V5);
        assert_eq!(category_of_tag("v5aru"), V5);
        assert_eq!(category_of_tag("v5r-i"), V5);
        assert_eq!(category_of_tag("vk"), VK);
        assert_eq!(category_of_tag("vs"), VS);
        assert_eq!(category_of_tag("vs-i"), VS);
        assert_eq!(category_of_tag("vs-s"), VS);
        assert_eq!(category_of_tag("adj-i"), ADJ_I);
        assert_eq!(category_of_tag("adj-ix"), ADJ_I);
        assert_eq!(category_of_tag("n"), 0);
        assert_eq!(category_of_tag("vi"), 0);
        assert_eq!(category_of_tag("adj-na"), 0);
    }
}
