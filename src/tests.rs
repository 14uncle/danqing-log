//! @author 十四叔
//! @date 2026/09/29
//!
//! main.rs 的测试模块 (2026-09-29 从 main.rs 原样搬出, 内容零修改)。

use super::*;
use std::io::Write;

// ─── v1x-licensing T4: 便携版激活接线 ───

/// 测试密钥对 (与 license.rs 的测试密钥对无关 —— 各测试域各自独立，
/// 谁也不是产品公钥)。
fn test_sign(payload_json: &str) -> (String, [u8; 32]) {
    use base64::Engine as _;
    use ed25519_dalek::{Signer, SigningKey};
    let sk = SigningKey::from_bytes(&[11u8; 32]);
    let sig = sk.sign(payload_json.as_bytes());
    let key = format!(
        "loglens1.{}.{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload_json.as_bytes()),
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sig.to_bytes())
    );
    (key, sk.verifying_key().to_bytes())
}

const LICENSE_PAYLOAD: &str = r#"{"v":1,"product":"danqing-log","tier":"personal","email":"t@e.st","issued_at":"1760000000","nonce":"ab"}"#;

/// 注入用临时配置路径 (license 落点在它的同名邻居; 并行 flake 教训：带 pid)。
fn temp_cfg_path(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "danqing-log-app-lic-{}-{tag}.toml",
        std::process::id()
    ))
}

#[test]
fn column_msgs_mutate_and_persist_state_account() {
    let cfg = temp_cfg_path("cols-msg");
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    // 功能已收付费 (gate-trio G2/G3/G4): 本锁验的是付费通路，注付费态;
    // 断言不动 (既有锁不许动语义)。
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    app.has_file = true;
    app.path = std::path::PathBuf::from("C:\\logs\\a.jsonl");
    app.schema = Some(Arc::new(jsonl::Schema {
        columns: vec![
            jsonl::Column {
                name: "a".into(),
                width_chars: 4,
            },
            jsonl::Column {
                name: "b".into(),
                width_chars: 4,
            },
        ],
    }));
    app.update(Msg::ColumnWidthSet("a".into(), 120.0));
    app.update(Msg::ColumnMoveBefore("b".into(), Some("a".into())));
    assert_eq!(app.columns.widths.get("a"), Some(&120.0));
    assert_eq!(
        app.columns.order,
        vec!["b".to_string(), "a".to_string()],
        "换位落点生效"
    );
    // 落盘 = state.json 邻居 (license.key 注入同规), 重启读得回
    let cols = cfg.with_extension("state.json");
    let loaded = danqing_log::columns::ColumnFiles::load_from(&cols);
    assert_eq!(
        loaded.get("C:\\logs\\a.jsonl").map(|c| &c.order),
        Some(&vec!["b".to_string(), "a".to_string()])
    );
    // 双击恢复 = 删手动宽覆盖
    app.update(Msg::ColumnWidthClear("a".into()));
    assert!(app.columns.widths.is_empty());
    std::fs::remove_file(&cols).ok();
    std::fs::remove_file(&cfg).ok();
}

/// T1 迁移 (SPEC-v1x-workspace-sessions D3): 旧 `columns.json` 在且新名无 →
/// 读旧零丢失; 落盘只写新名; 新名在则新名优先 (旧名残留不再被读)。
#[test]
fn state_account_migrates_from_legacy_columns_json() {
    let cfg = temp_cfg_path("migrate");
    let legacy = cfg.with_extension("columns.json");
    let newp = cfg.with_extension("state.json");
    let p = temp_log(b"{\"level\":\"ERROR\"}\n{\"level\":\"WARN\"}\n{\"level\":\"INFO\"}\n{\"level\":\"DEBUG\"}\n{\"level\":\"TRACE\"}\n{\"level\":\"FATAL\"}\n");
    let path_str = "C:\\logs\\m.log";
    std::fs::write(
        &legacy,
        serde_json::to_vec_pretty(&serde_json::json!({
            "files": [ {
                "path": path_str, "updated": 1, "order": ["a", "b"],
                "hidden": ["b"], "widths": {"a": 90.0}, "bookmarks": [5, 2]
            } ],
            "sessions": [ {
                "path": path_str, "name": "旧台", "filter": "a=1", "search": "x",
                "order": ["a"], "expands": [1], "updated": 2
            } ]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    // 功能已收付费 (gate-trio G2/G3/G4): 本锁验的是付费通路，注付费态;
    // 断言不动 (既有锁不许动语义)。
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    app.has_file = true;
    app.path = std::path::PathBuf::from(path_str);
    app.file = Arc::new(LogFile::open(&p).unwrap());
    app.load_state_for_current_file();
    assert_eq!(app.columns.order, vec!["a".to_string(), "b".to_string()]);
    assert!(app.columns.is_hidden("b"));
    assert_eq!(app.columns.widths.get("a"), Some(&90.0));
    assert_eq!(
        app.bookmarks.iter().copied().collect::<Vec<_>>(),
        vec![2, 5],
        "旧账本读入零丢失"
    );
    // 落盘只写新名 (sessions 段随读改写保真)
    assert!(app.save_state());
    assert!(newp.exists(), "落盘写新名");
    let saved = danqing_log::columns::ColumnFiles::load_from(&newp);
    assert_eq!(
        saved.sessions_for_path(path_str).len(),
        1,
        "sessions 迁移保真"
    );
    // 新名在 → 新名优先：旧名再动不进视野
    std::fs::write(&legacy, b"{ not json").unwrap();
    app.load_state_for_current_file();
    assert_eq!(app.columns.order, vec!["a".to_string(), "b".to_string()]);
    std::fs::remove_file(&legacy).ok();
    std::fs::remove_file(&newp).ok();
    std::fs::remove_file(&cfg).ok();
    std::fs::remove_file(&p).ok();
}

/// 家法随迁 (panic 封死): 测试构建无注入路径不得读写真实状态账本。
#[test]
#[should_panic(expected = "测试不得读写真实")]
fn state_path_without_injection_panics_in_tests() {
    let mut app = LogApp::new_empty_at(None);
    app.has_file = true;
    let _ = app.save_state();
}

// ---- T2: 会话 save/apply/delete 状态链 (SPEC-v1x-workspace-sessions) ----

/// 四样载荷 save→apply roundtrip (含写穿实证) + **书签零触碰** (Open Q1) +
/// 换路径切片替换清选中。
#[test]
fn session_save_apply_roundtrips_four_parts_and_touches_no_bookmarks() {
    let cfg = temp_cfg_path("sess-sa");
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    let p = temp_log(
            b"{\"a\":{\"b\":1},\"level\":\"ERROR\"}\nplain line\n{\"a\":{\"c\":2},\"level\":\"WARN\"}\n",
        );
    app.has_file = true;
    app.path = std::path::PathBuf::from("C:\\logs\\s.log");
    app.file = Arc::new(LogFile::open(&p).unwrap());
    app.schema = Some(Arc::new(jsonl::Schema {
        columns: vec![jsonl::Column {
            name: "level".into(),
            width_chars: 8,
        }],
    }));
    app.load_state_for_current_file();
    // 调好工作台：摆列 + 过滤/搜索串 + 展开 0 号行; 书签在场 (零触碰判据)
    app.update(Msg::ColumnWidthSet("level".into(), 200.0));
    app.filter_applied = "level=ERROR".into();
    app.search_query = "plain".into();
    app.toggle_expand(0);
    app.bookmarks.insert(2);
    let bookmarks_before = app.bookmarks.clone();
    app.update(Msg::SaveSession("排障 A".into()));
    assert_eq!(app.sessions.len(), 1);
    assert_eq!(app.sessions[0].name, "排障 A");
    assert_eq!(app.sessions[0].expands, vec![0], "快照 = 展开行号表");
    // 破坏现场后应用 = 四样回来
    app.update(Msg::ColumnWidthClear("level".into()));
    app.filter_applied.clear();
    app.search_query.clear();
    app.toggle_expand(0);
    app.update(Msg::ApplySession("排障 A".into()));
    assert_eq!(app.filter_applied, "level=ERROR", "过滤串回来");
    assert_eq!(app.search_query, "plain", "搜索串回来");
    assert_eq!(app.columns.widths.get("level"), Some(&200.0), "列摆法回来");
    assert!(app.expanded.is_expanded(0), "展开重建");
    assert_eq!(
        app.session_selected.as_deref(),
        Some("排障 A"),
        "应用记选中"
    );
    assert_eq!(app.bookmarks, bookmarks_before, "书签零触碰 (Open Q1)");
    // 写穿实证 (摘写穿 = 本断言红): files 段条目 = 会话列摆法
    let saved = danqing_log::columns::ColumnFiles::load_from(&cfg.with_extension("state.json"));
    assert_eq!(
        saved
            .get_entry("C:\\logs\\s.log")
            .and_then(|e| e.config.widths.get("level")),
        Some(&200.0),
        "应用写穿 per-file 条目"
    );
    // 换路径 = 切片替换 + 清选中 (他路径列表只显自己的，Open Q3)
    app.path = std::path::PathBuf::from("C:\\logs\\other.log");
    app.load_state_for_current_file();
    assert!(app.sessions.is_empty(), "换路径 = 该路径的会话");
    assert!(app.session_selected.is_none());
    std::fs::remove_file(cfg.with_extension("state.json")).ok();
    std::fs::remove_file(&cfg).ok();
    std::fs::remove_file(&p).ok();
}

/// 展开重建剔除 (D6): 越界行号 / 不可展开行**静默剔除** (书签越界剔除同哲学)。
#[test]
fn session_apply_prunes_invalid_expands() {
    let cfg = temp_cfg_path("sess-prune");
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    let p = temp_log(b"{\"a\":{\"b\":1}}\nplain\n");
    app.has_file = true;
    app.path = std::path::PathBuf::from("C:\\logs\\p.log");
    app.file = Arc::new(LogFile::open(&p).unwrap());
    app.load_state_for_current_file();
    app.sessions.push(danqing_log::columns::SessionEntry {
        path: "C:\\logs\\p.log".into(),
        name: "越界台".into(),
        filter: String::new(),
        search: String::new(),
        config: danqing_log::columns::ColumnConfig::default(),
        expands: vec![0, 1, 99],
        merge: None,
        updated: 1,
    });
    app.update(Msg::ApplySession("越界台".into()));
    assert!(app.expanded.is_expanded(0), "合法行重建");
    assert!(!app.expanded.is_expanded(1), "不可展开行剔除 (parse 失败)");
    assert!(!app.expanded.is_expanded(99), "越界行号剔除");
    std::fs::remove_file(cfg.with_extension("state.json")).ok();
    std::fs::remove_file(&cfg).ok();
    std::fs::remove_file(&p).ok();
}

/// D6 守卫：空名拒绝 / 满 32 拒绝 (新名才计数) / 同名覆盖不算新增 /
/// 删除后再存; 未知名三动作说清不动账。
#[test]
fn session_save_rejects_empty_and_full_cap_and_overwrites() {
    let cfg = temp_cfg_path("sess-cap");
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    let p = temp_log(b"{\"a\":{\"b\":1}}\n");
    app.has_file = true;
    app.path = std::path::PathBuf::from("C:\\logs\\c.log");
    app.file = Arc::new(LogFile::open(&p).unwrap());
    app.load_state_for_current_file();
    app.update(Msg::SaveSession("   ".into()));
    assert!(app.sessions.is_empty(), "空名拒绝");
    assert!(app.notice.is_some(), "拒绝说清");
    for i in 0..danqing_log::columns::MAX_SESSIONS_PER_PATH {
        app.update(Msg::SaveSession(format!("s{i:02}")));
    }
    assert_eq!(
        app.sessions.len(),
        danqing_log::columns::MAX_SESSIONS_PER_PATH
    );
    app.update(Msg::SaveSession("s99".into()));
    assert_eq!(
        app.sessions.len(),
        danqing_log::columns::MAX_SESSIONS_PER_PATH,
        "满员拒绝"
    );
    // 同名覆盖不算新增 + 快照更新
    app.filter_applied = "x=1".into();
    app.update(Msg::SaveSession("s00".into()));
    assert_eq!(
        app.sessions.len(),
        danqing_log::columns::MAX_SESSIONS_PER_PATH
    );
    assert_eq!(
        app.sessions
            .iter()
            .find(|s| s.name == "s00")
            .map(|s| s.filter.as_str()),
        Some("x=1"),
        "同名覆盖生效"
    );
    // 删除选中后再存 + 删除落盘 + 未知名说清不动账
    app.session_selected = Some("s00".into());
    app.update(Msg::DeleteSelectedSession);
    assert_eq!(
        app.sessions.len(),
        danqing_log::columns::MAX_SESSIONS_PER_PATH - 1
    );
    assert!(app.session_selected.is_none(), "删选中 = 清指针");
    let saved = danqing_log::columns::ColumnFiles::load_from(&cfg.with_extension("state.json"));
    assert!(
        saved
            .sessions_for_path("C:\\logs\\c.log")
            .iter()
            .all(|s| s.name != "s00"),
        "删除落盘"
    );
    app.update(Msg::SaveSession("s99".into()));
    assert_eq!(
        app.sessions.len(),
        danqing_log::columns::MAX_SESSIONS_PER_PATH
    );
    let before = app.sessions.len();
    app.session_selected = Some("ghost".into());
    app.update(Msg::DeleteSelectedSession);
    app.update(Msg::ApplySession("ghost".into()));
    assert_eq!(app.sessions.len(), before, "未知名不动账");
    assert!(app.notice.is_some(), "未知名说清");
    // 无选中点删除 = 提示不动账
    app.session_selected = None;
    app.update(Msg::DeleteSelectedSession);
    assert_eq!(app.sessions.len(), before, "无选中不动账");
    // 幽灵名删除 = 说清 + 指针随名清 (评审 M9)
    app.session_selected = Some("ghost".into());
    app.update(Msg::DeleteSelectedSession);
    assert!(app.session_selected.is_none(), "幽灵名删除也清指针");
    std::fs::remove_file(cfg.with_extension("state.json")).ok();
    std::fs::remove_file(&cfg).ok();
    std::fs::remove_file(&p).ok();
}

/// D4 门控两态 (两道闸): 免费态入口/三动作全拦 (升级提示 + 账本零变化);
/// 付费态放行且**永不触发升级提示**。
#[test]
fn session_gate_blocks_free_tier_and_never_prompts_paid() {
    let cfg = temp_cfg_path("sess-gate");
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    let p = temp_log(b"{\"a\":{\"b\":1}}\n");
    app.has_file = true;
    app.path = std::path::PathBuf::from("C:\\logs\\g.log");
    app.file = Arc::new(LogFile::open(&p).unwrap());
    // 免费态：入口被拦，弹层不开
    app.update(Msg::OpenSessionMenu);
    assert_eq!(
        app.upgrade_prompt,
        Some(Feature::WorkspaceSessions),
        "免费态入口 = 升级提示"
    );
    assert!(!app.session_menu_open, "免费态不得开弹层");
    // 免费态：三动作兜底闸全拦 (账本零变化)
    app.update(Msg::SaveSession("s".into()));
    app.update(Msg::ApplySession("s".into()));
    app.update(Msg::DeleteSelectedSession);
    assert!(app.sessions.is_empty(), "免费态动作零落账");
    assert!(!cfg.with_extension("state.json").exists(), "免费态零落盘");
    // 付费态：放行; 全程永不触发升级提示 (两道闸锁)
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    app.upgrade_prompt = None;
    app.update(Msg::OpenSessionMenu);
    assert!(app.session_menu_open, "付费态开弹层");
    assert!(app.upgrade_prompt.is_none(), "付费态永不误弹");
    app.update(Msg::SaveSession("s".into()));
    app.update(Msg::ApplySession("s".into()));
    app.session_selected = Some("s".into());
    app.update(Msg::DeleteSelectedSession);
    assert!(app.sessions.is_empty(), "付费态动作放行");
    assert!(app.upgrade_prompt.is_none(), "付费态动作也不误弹");
    std::fs::remove_file(cfg.with_extension("state.json")).ok();
    std::fs::remove_file(&cfg).ok();
    std::fs::remove_file(&p).ok();
}

/// 弹层族第四员全套：Esc 次序首插 / 与弹层族互斥 / 模态门禁 (导航键 + 滚轮
/// 不穿) / 关弹层清草稿 (session_clear_rev)。
#[test]
fn session_menu_esc_mutex_modal_and_draft_clear() {
    let cfg = temp_cfg_path("sess-wire");
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    let p = temp_log(b"{\"a\":{\"b\":1}}\nplain\n");
    app.has_file = true;
    app.path = std::path::PathBuf::from("C:\\logs\\w.log");
    app.file = Arc::new(LogFile::open(&p).unwrap());
    // Esc 次序：会话先于 picker/col (D5 插层)
    app.session_menu_open = true;
    app.picker_open = true;
    let esc = Event::Key {
        key: Key::Named(NamedKey::Escape),
        pressed: true,
        shift: false,
        ctrl: false,
        alt: false,
    };
    assert!(matches!(
        app.app_key_filter(&esc),
        Some(Msg::CloseSessionMenu)
    ));
    // 互斥：开 picker 关会话; 开会话关弹层族
    app.session_menu_open = false;
    app.picker_open = true;
    app.col_menu_open = true;
    let rev = app.session_clear_rev;
    app.update(Msg::OpenSessionMenu);
    assert!(app.session_menu_open && !app.picker_open && !app.col_menu_open);
    assert!(app.session_clear_rev > rev, "开弹层清命名草稿");
    // 模态门禁：会话开时 ↓/滚轮不穿到日志 (滚轮用**可动**位：top_row=0 时
    // 向上滚会被钳回 0 —— 家族⑥假绿，评审 M5 抓出后改真锁)
    app.top_row = 5.0;
    app.event(&Event::Key {
        key: Key::Named(NamedKey::ArrowDown),
        pressed: true,
        shift: false,
        ctrl: false,
        alt: false,
    });
    assert_eq!(app.top_row, 5.0, "会话开着 ↓ 不穿");
    let before = app.top_row;
    app.event(&Event::MouseWheel {
        delta: (0.0, -1.0),
        position: danqing::Point::new(400.0, 300.0),
        shift: false,
        ctrl: false,
        alt: false,
    });
    assert_eq!(app.top_row, before, "会话开着滚轮不穿");
    // 关弹层清草稿
    let rev = app.session_clear_rev;
    app.update(Msg::CloseSessionMenu);
    assert!(!app.session_menu_open);
    assert!(app.session_clear_rev > rev, "关弹层清命名草稿");
    // 互斥关也清草稿 (评审 M12: close_popovers 随关走，不靠下次开兜)
    app.update(Msg::OpenSessionMenu);
    let rev = app.session_clear_rev;
    app.update(Msg::OpenPicker);
    assert!(!app.session_menu_open && app.picker_open);
    assert!(app.session_clear_rev > rev, "互斥关会话也清草稿");
    std::fs::remove_file(&cfg).ok();
    std::fs::remove_file(&p).ok();
}

/// 评审 M1/M2: 损坏判据认 sessions 段 (sessions-only = 可辨，不备份不丢他会话);
/// 空/坏新名回落旧名 (读备判据同源); 保存后旧记忆写回 + 旧名**退役一次性**。
#[test]
fn state_account_recognizes_sessions_and_falls_back_then_retires_legacy() {
    let cfg = temp_cfg_path("m12");
    let legacy = cfg.with_extension("columns.json");
    let newp = cfg.with_extension("state.json");
    let bak = cfg.with_extension("state.json.bak");
    let retired = cfg.with_extension("columns.json.migrated");
    let p = temp_log(b"{\"a\":{\"b\":1}}\n");
    let path_str = "C:\\logs\\m12.log";
    // M1: files 全废 + sessions 完好 = 丢段不丢账的合法容错 → 不算损坏
    std::fs::write(
        &newp,
        serde_json::to_vec_pretty(&serde_json::json!({
            "files": [ { "path": 1 }, { "order": ["a"] } ],
            "sessions": [
                { "path": "C:\\logs\\z.log", "name": "他路径台", "updated": 1, "order": ["x"] }
            ]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    // 功能已收付费 (gate-trio G2/G3/G4): 本锁验的是付费通路，注付费态;
    // 断言不动 (既有锁不许动语义)。
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    app.has_file = true;
    app.path = std::path::PathBuf::from(path_str);
    app.file = Arc::new(LogFile::open(&p).unwrap());
    assert!(app.save_state());
    assert!(!bak.exists(), "sessions-only 不算损坏 (M1)");
    let acc = danqing_log::columns::ColumnFiles::load_from(&newp);
    assert_eq!(
        acc.sessions_for_path("C:\\logs\\z.log").len(),
        1,
        "他路径会话不丢"
    );
    std::fs::remove_file(&newp).ok();
    // M2: 旧名有货 + 新名空 → 回落旧名; 保存后记忆写回新名，旧名退役
    std::fs::write(
        &legacy,
        serde_json::to_vec_pretty(&serde_json::json!({
            "files": [ {
                "path": path_str, "updated": 1, "order": ["a"],
                "hidden": ["a"], "widths": {}, "bookmarks": [0]
            } ],
            "sessions": [
                { "path": path_str, "name": "旧台", "updated": 2, "order": ["a"] }
            ]
        }))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(&newp, b"").unwrap(); // 空新名：不许挡死迁移 (M2)
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    // 同上：迁移/回落实验跑在付费态 (断言不动)
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    app.has_file = true;
    app.path = std::path::PathBuf::from(path_str);
    app.file = Arc::new(LogFile::open(&p).unwrap());
    app.load_state_for_current_file();
    assert!(app.columns.is_hidden("a"), "空新名回落旧名");
    assert_eq!(app.sessions.len(), 1);
    assert!(app.save_state());
    let acc = danqing_log::columns::ColumnFiles::load_from(&newp);
    assert!(
        acc.get_entry(path_str).unwrap().config.is_hidden("a"),
        "旧记忆写回新账，不被空内存覆盖"
    );
    assert_eq!(acc.sessions_for_path(path_str).len(), 1, "旧会话写回");
    assert!(retired.exists() && !legacy.exists(), "迁移一次性：旧名退役");
    std::fs::remove_file(&legacy).ok();
    std::fs::remove_file(&newp).ok();
    std::fs::remove_file(&bak).ok();
    std::fs::remove_file(&retired).ok();
    std::fs::remove_file(&cfg).ok();
    std::fs::remove_file(&p).ok();
}

/// 评审 M7: `clear_search` 作废在途搜索 (`clear_filter` 同规) —— 清空/
/// 空搜索会话应用后，旧搜索不得经 tick 拾取复活。
#[test]
fn clear_search_kills_pending_search_job() {
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("sclr")));
    app.has_file = true;
    let p = temp_log(b"timeout\nplain\n");
    app.file = Arc::new(LogFile::open(&p).unwrap());
    app.apply_search("timeout".to_string());
    app.clear_search();
    assert!(app.search_query.is_empty());
    std::thread::sleep(std::time::Duration::from_millis(100));
    let mut resurrected = false;
    for _ in 0..50 {
        if app.search_job.poll().is_some() {
            resurrected = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(!resurrected, "invalidate 后在途搜索结果必须被丢弃");
    assert!(app.search.is_none() && app.search_query.is_empty());
    std::fs::remove_file(&p).ok();
}

/// 评审 M8: 会话搜索串正则无效 (跨编码/手造) → **显式清空** + 说清，
/// 其余三样照常应用 —— 四样必须「已应用或已显式清空」, 不许 3/4 谎报成功。
#[test]
fn apply_session_with_invalid_search_explicitly_clears() {
    let cfg = temp_cfg_path("sess-regex");
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    let p = temp_log(b"{\"level\":\"ERROR\"}\nplain\n");
    app.has_file = true;
    app.path = std::path::PathBuf::from("C:\\logs\\r.log");
    app.file = Arc::new(LogFile::open(&p).unwrap());
    app.load_state_for_current_file();
    app.search_query = "旧搜索".into();
    app.sessions.push(danqing_log::columns::SessionEntry {
        path: "C:\\logs\\r.log".into(),
        name: "坏正则台".into(),
        filter: "level=ERROR".into(),
        search: "(".into(), // UTF-8 下 `(?i)(` 正则无效
        config: danqing_log::columns::ColumnConfig::default(),
        expands: Vec::new(),
        merge: None,
        updated: 1,
    });
    app.update(Msg::ApplySession("坏正则台".into()));
    assert!(app.search_query.is_empty(), "无效搜索串 = 显式清空");
    assert!(app.notice.is_some(), "说清为什么清");
    assert_eq!(app.filter_applied, "level=ERROR", "其余三样照常应用");
    std::fs::remove_file(cfg.with_extension("state.json")).ok();
    std::fs::remove_file(&cfg).ok();
    std::fs::remove_file(&p).ok();
}

/// 评审 M6: 弹层互斥**双向** —— 他弹层开会话必关 (漏进 close_popovers 就红)。
#[test]
fn session_menu_mutex_is_bidirectional() {
    let cfg = temp_cfg_path("sess-mutex");
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    for opener in [Msg::OpenPicker, Msg::OpenColMenu, Msg::OpenSettings] {
        app.update(Msg::OpenSessionMenu);
        assert!(app.session_menu_open);
        app.update(opener);
        assert!(!app.session_menu_open, "开他弹层/设置必关会话");
    }
    std::fs::remove_file(&cfg).ok();
}

/// T5: 列管理开关/恢复默认/≥1 可见守卫 + 落盘; 与导出菜单互斥。
#[test]
fn col_menu_toggle_reset_guard_and_persist() {
    let cfg = temp_cfg_path("cols-menu");
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    // 功能已收付费 (gate-trio G2/G3/G4): 本锁验的是付费通路，注付费态;
    // 断言不动 (既有锁不许动语义)。
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    app.has_file = true;
    app.path = std::path::PathBuf::from("C:\\logs\\m.jsonl");
    app.schema = Some(Arc::new(jsonl::Schema {
        columns: vec![
            jsonl::Column {
                name: "a".into(),
                width_chars: 4,
            },
            jsonl::Column {
                name: "b".into(),
                width_chars: 4,
            },
        ],
    }));
    // 互斥：开列管理关导出菜单，反之亦然
    app.export_menu_open = true;
    app.update(Msg::OpenColMenu);
    assert!(app.col_menu_open && !app.export_menu_open);
    app.update(Msg::ToggleColumn("a".into()));
    assert!(app.columns.is_hidden("a"));
    // ≥1 可见守卫 (D6): 关最后一可见列拒绝 + 提示 + 零变更
    app.update(Msg::ToggleColumn("b".into()));
    assert!(!app.columns.is_hidden("b"), "最后一可见列不可藏");
    assert!(
        app.notice.as_ref().is_some_and(
            |(t, k)| t.contains("至少保留一列") && matches!(k, crate::NoticeKind::Warn)
        ),
        "拒绝须说清为什么"
    );
    // 恢复默认 (D3)
    app.update(Msg::ResetColumns);
    assert_eq!(app.columns.order, vec!["a".to_string(), "b".to_string()]);
    assert!(app.columns.hidden.is_empty());
    // 落盘读回 (D4)
    let cols = cfg.with_extension("state.json");
    let loaded = danqing_log::columns::ColumnFiles::load_from(&cols);
    assert!(loaded.get("C:\\logs\\m.jsonl").is_some());
    std::fs::remove_file(&cols).ok();
    std::fs::remove_file(&cfg).ok();
}

/// T5: 换文件载入 per-路径记忆 (D4) + 关列管理菜单 (R4 先例同款)。
#[test]
fn apply_fresh_loads_per_path_memory_and_closes_col_menu() {
    let cfg = temp_cfg_path("cols-fresh");
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    // 功能已收付费 (gate-trio G2/G3/G4): 本锁验的是付费通路，注付费态;
    // 断言不动 (既有锁不许动语义)。
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    let schema = || {
        Some(jsonl::Schema {
            columns: vec![
                jsonl::Column {
                    name: "a".into(),
                    width_chars: 4,
                },
                jsonl::Column {
                    name: "b".into(),
                    width_chars: 4,
                },
            ],
        })
    };
    let p = temp_log(b"x\n");
    // 路径 A: 摆一列手动宽
    let out_a = OpenOutcome {
        file: LogFile::open(&p).unwrap(),
        schema: schema(),
        incremental_hits: None,
        rebuilt: false,
        level_column: None,
    };
    app.apply_fresh(std::path::PathBuf::from("A.jsonl"), out_a);
    app.update(Msg::ColumnWidthSet("a".into(), 222.0));
    app.col_menu_open = true;
    // 换文件 B: 菜单关 + 摠法回默认 (B 无记忆)
    let out_b = OpenOutcome {
        file: LogFile::open(&p).unwrap(),
        schema: schema(),
        incremental_hits: None,
        rebuilt: false,
        level_column: None,
    };
    app.apply_fresh(std::path::PathBuf::from("B.jsonl"), out_b);
    assert!(!app.col_menu_open, "换文件关列管理菜单");
    assert!(app.columns.widths.is_empty(), "B 无记忆 = 默认摆法");
    // 回 A: 记忆回来
    let out_a2 = OpenOutcome {
        file: LogFile::open(&p).unwrap(),
        schema: schema(),
        incremental_hits: None,
        rebuilt: false,
        level_column: None,
    };
    app.apply_fresh(std::path::PathBuf::from("A.jsonl"), out_a2);
    assert_eq!(
        app.columns.widths.get("a"),
        Some(&222.0),
        "per-路径记忆读回"
    );
    std::fs::remove_file(cfg.with_extension("state.json")).ok();
    std::fs::remove_file(&cfg).ok();
    std::fs::remove_file(&p).ok();
}

/// T5: Esc 次序 —— 列管理先于导出格式菜单 (D3 插层)。
#[test]
fn esc_prefers_col_menu_over_export_menu() {
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("cols-esc")));
    app.col_menu_open = true;
    app.export_menu_open = true;
    let esc = Event::Key {
        key: Key::Named(NamedKey::Escape),
        pressed: true,
        shift: false,
        ctrl: false,
        alt: false,
    };
    assert!(matches!(app.app_key_filter(&esc), Some(Msg::CloseColMenu)));
    std::fs::remove_file(temp_cfg_path("cols-esc")).ok();
}

/// T5: 模态守卫 —— 列管理开着时全局 Ctrl 键不穿到菜单后 (P32 同纪律)。
/// 探针取 Ctrl+L (侧栏); **绝不碰 Ctrl+O** (会弹真文件对话框 —— 测试严禁真实桌面副作用)。
#[test]
fn modal_guard_blocks_globals_while_col_menu_open() {
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("cols-modal")));
    let ctrl_l = Event::Key {
        key: Key::Character("l".to_string()),
        pressed: true,
        shift: false,
        ctrl: true,
        alt: false,
    };
    assert!(matches!(
        app.app_key_filter(&ctrl_l),
        Some(Msg::ToggleHistogram)
    ));
    app.col_menu_open = true;
    assert!(matches!(app.app_key_filter(&ctrl_l), Some(Msg::Noop)));
    // 剪辑键放行 (评审 Critical 先例)
    let ctrl_v = Event::Key {
        key: Key::Character("v".to_string()),
        pressed: true,
        shift: false,
        ctrl: true,
        alt: false,
    };
    assert!(app.app_key_filter(&ctrl_v).is_none());
    std::fs::remove_file(temp_cfg_path("cols-modal")).ok();
}

/// T5/D5 回归锁：**显示配置不影响交付物** —— 摆列/隐藏后 CSV 列仍 schema 首见序全列。
#[test]
fn csv_columns_ignore_column_config() {
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("cols-csv")));
    app.has_file = true;
    app.path = std::path::PathBuf::from("C:\\logs\\c.jsonl");
    app.schema = Some(Arc::new(jsonl::Schema {
        columns: vec![
            jsonl::Column {
                name: "a".into(),
                width_chars: 4,
            },
            jsonl::Column {
                name: "b".into(),
                width_chars: 4,
            },
        ],
    }));
    app.update(Msg::ColumnMoveBefore("b".into(), Some("a".into())));
    app.update(Msg::ToggleColumn("a".into()));
    assert_eq!(
        app.export_csv_columns(),
        vec!["a".to_string(), "b".to_string()],
        "摆列不改变导出列 (export D5)"
    );
    std::fs::remove_file(temp_cfg_path("cols-csv").with_extension("state.json")).ok();
    std::fs::remove_file(temp_cfg_path("cols-csv")).ok();
}

// ---- 评审修复锁 (2026-09-23 双路评审并账) ----

/// 评审 C4: `apply_rebuild` 也须对账列配置 —— 轮转/截断后 schema 可能整体换，
/// 旧摆法不 merge 会残留失配序 (含 C2 的 0 列面); 与 `apply_fresh` 同纪律。
#[test]
fn apply_rebuild_merges_column_config_with_new_schema() {
    let cfg = temp_cfg_path("cols-rebuild");
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    app.has_file = true;
    app.path = std::path::PathBuf::from("C:\\logs\\r.jsonl");
    app.schema = Some(Arc::new(jsonl::Schema {
        columns: vec![
            jsonl::Column {
                name: "a".into(),
                width_chars: 4,
            },
            jsonl::Column {
                name: "b".into(),
                width_chars: 4,
            },
            jsonl::Column {
                name: "c".into(),
                width_chars: 4,
            },
        ],
    }));
    // 摆法：藏 a、b, 只留 c 可见 (落盘形态同款的脏态)
    app.update(Msg::ToggleColumn("a".into()));
    app.update(Msg::ToggleColumn("b".into()));
    assert_eq!(app.columns.visible_names().collect::<Vec<_>>(), vec!["c"]);
    // 轮转 rebuild: 新 schema 只剩 a,b (c 没了)
    let p = temp_log(b"x\n");
    let out = OpenOutcome {
        file: LogFile::open(&p).unwrap(),
        schema: Some(jsonl::Schema {
            columns: vec![
                jsonl::Column {
                    name: "a".into(),
                    width_chars: 4,
                },
                jsonl::Column {
                    name: "b".into(),
                    width_chars: 4,
                },
            ],
        }),
        incremental_hits: None,
        rebuilt: true,
        level_column: None,
    };
    app.apply_rebuild(std::path::PathBuf::from("C:\\logs\\r.jsonl").as_path(), out);
    assert_eq!(
        app.columns.order,
        vec!["a".to_string(), "b".to_string()],
        "rebuild 后列配置须对账新 schema"
    );
    assert_eq!(
        app.columns.visible_names().collect::<Vec<_>>(),
        vec!["a"],
        "唯一可见列被剔除后 merge 兜底 (≥1 可见)"
    );
    std::fs::remove_file(cfg.with_extension("state.json")).ok();
    std::fs::remove_file(&cfg).ok();
    std::fs::remove_file(&p).ok();
}

/// 评审 Optional (五轴⑥): 未知列名的 Toggle 不得误报「至少保留一列可见」守卫文案。
#[test]
fn toggle_unknown_column_reports_nothing() {
    let cfg = temp_cfg_path("cols-ghost");
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    app.has_file = true;
    app.path = std::path::PathBuf::from("C:\\logs\\g.jsonl");
    app.schema = Some(Arc::new(jsonl::Schema {
        columns: vec![jsonl::Column {
            name: "a".into(),
            width_chars: 4,
        }],
    }));
    app.update(Msg::ToggleColumn("ghost".into()));
    assert!(app.notice.is_none(), "未知列零动作，不得报守卫文案");
    std::fs::remove_file(cfg.with_extension("state.json")).ok();
    std::fs::remove_file(&cfg).ok();
}

// ---- T2: 书签持久化全链路 (SPEC-v1x-bookmark-persist D1/D2/D3) ----

/// toggle 增删即落盘 + 重读 (`load_state_for_current_file`) 恢复 (T2①⑥)。
/// 摘 toggle 的 save_state / 摘载入的 bookmarks 读取 → 本锁红 (A/B)。
#[test]
fn toggle_bookmark_persists_and_reloads() {
    let cfg = temp_cfg_path("bm-toggle");
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    app.has_file = true;
    app.path = std::path::PathBuf::from("C:\\logs\\bm.log");
    let p = temp_log(b"l0\nl1\nl2\n");
    app.file = Arc::new(LogFile::open(&p).unwrap());
    // 「增即落盘」是**付费通路**语义 (gate-trio G3 书签持久化已收付费):
    // 本锁验的是持久化，按注入惯例注付费态; 断言不动。
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    app.selected = 1;
    app.toggle_bookmark();
    assert!(app.bookmarks.contains(&1), "toggle 添加");
    let cols = cfg.with_extension("state.json");
    let loaded = danqing_log::columns::ColumnFiles::load_from(&cols);
    assert_eq!(
        loaded
            .get_entry("C:\\logs\\bm.log")
            .map(|e| e.bookmarks.as_slice()),
        Some([1u64].as_slice()),
        "增即落盘"
    );
    // 内存清空后重读 = 恢复
    app.bookmarks.clear();
    app.load_state_for_current_file();
    assert!(app.bookmarks.contains(&1), "重读恢复");
    // 再点 = 去掉，同步落盘
    app.selected = 1;
    app.toggle_bookmark();
    assert!(!app.bookmarks.contains(&1), "toggle 去掉");
    let loaded = danqing_log::columns::ColumnFiles::load_from(&cols);
    assert!(
        loaded
            .get_entry("C:\\logs\\bm.log")
            .is_some_and(|e| e.bookmarks.is_empty()),
        "删即落盘"
    );
    std::fs::remove_file(&cols).ok();
    std::fs::remove_file(&cfg).ok();
    std::fs::remove_file(&p).ok();
}

/// per-路径隔离 (T2②) + apply_fresh 全链 (T2⑤, plan 核实⑤次序陷阱防漏锁):
/// 换文件 = 载入**替换**语义 —— A 的书签不带进 B, B 的记忆恢复
/// (若 `bookmarks.clear()` 留在载入之后，这里会得到空集)。
#[test]
fn bookmarks_are_per_path_and_apply_fresh_replaces_with_memory() {
    let cfg = temp_cfg_path("bm-path");
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    // 功能已收付费 (gate-trio G2/G3/G4): 本锁验的是付费通路，注付费态;
    // 断言不动 (既有锁不许动语义)。
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    let p = temp_log(b"x\ny\n"); // 2 行
    let out = || OpenOutcome {
        file: LogFile::open(&p).unwrap(),
        schema: None,
        incremental_hits: None,
        rebuilt: false,
        level_column: None,
    };
    // 路径 A: 夹书签落盘
    app.apply_fresh(std::path::PathBuf::from("A.log"), out());
    app.selected = 0;
    app.toggle_bookmark();
    assert!(app.bookmarks.contains(&0));
    // 路径 B: 手造记忆 [1]
    let cols = cfg.with_extension("state.json");
    let mut files = danqing_log::columns::ColumnFiles::load_from(&cols);
    files.put(danqing_log::columns::FileEntry {
        path: "B.log".into(),
        config: danqing_log::columns::ColumnConfig::default(),
        bookmarks: vec![1],
        updated: 999,
    });
    files.save_to(&cols).unwrap();
    // 换 B: 无 A 残留 + B 记忆恢复
    app.apply_fresh(std::path::PathBuf::from("B.log"), out());
    assert_eq!(
        app.bookmarks.iter().copied().collect::<Vec<_>>(),
        vec![1],
        "B 记忆恢复且 A 不跟过来 (载入替换 clear)"
    );
    // 回 A: 又在
    app.apply_fresh(std::path::PathBuf::from("A.log"), out());
    assert!(app.bookmarks.contains(&0), "A 记忆回来");
    std::fs::remove_file(&cols).ok();
    std::fs::remove_file(&cfg).ok();
    std::fs::remove_file(&p).ok();
}

/// 越界剔除 (T2③, 载入侧): 行号 ≥ line_count 消失、界内保留; **不写回**磁盘
/// (损坏零写回同哲学)。
#[test]
fn load_state_drops_out_of_bounds_bookmarks() {
    let cfg = temp_cfg_path("bm-oob");
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    // 功能已收付费 (gate-trio G2/G3/G4): 本锁验的是付费通路，注付费态;
    // 断言不动 (既有锁不许动语义)。
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    app.has_file = true;
    app.path = std::path::PathBuf::from("C:\\logs\\oob.log");
    let p = temp_log(b"l0\nl1\nl2\n"); // 3 行
    app.file = Arc::new(LogFile::open(&p).unwrap());
    let cols = cfg.with_extension("state.json");
    let mut files = danqing_log::columns::ColumnFiles::default();
    files.put(danqing_log::columns::FileEntry {
        path: "C:\\logs\\oob.log".into(),
        config: danqing_log::columns::ColumnConfig::default(),
        bookmarks: vec![0, 5, 99],
        updated: 1,
    });
    files.save_to(&cols).unwrap();
    app.load_state_for_current_file();
    assert_eq!(
        app.bookmarks.iter().copied().collect::<Vec<_>>(),
        vec![0],
        "5/99 越界剔除，0 保留"
    );
    let on_disk = danqing_log::columns::ColumnFiles::load_from(&cols);
    assert_eq!(
        on_disk
            .get_entry("C:\\logs\\oob.log")
            .map(|e| e.bookmarks.as_slice()),
        Some([0u64, 5, 99].as_slice()),
        "剔除不写回"
    );
    std::fs::remove_file(&cols).ok();
    std::fs::remove_file(&cfg).ok();
    std::fs::remove_file(&p).ok();
}

/// 上限守卫 (T2④, D3): 满 `MAX_BOOKMARKS` 拒绝新增 + 说清为什么 + 零变更;
/// **删除照常**（满员时 toggle 已有书签仍须能删 —— 评审 R②: 别让守卫顺序
/// 把「腾空位」的唯一路径堵死）; 去掉一个又能加。全走 toggle 行为，不直改集合。
#[test]
fn toggle_bookmark_refuses_at_cap_with_notice() {
    let cfg = temp_cfg_path("bm-cap");
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    app.has_file = true;
    app.path = std::path::PathBuf::from("C:\\logs\\cap.log");
    // 257 行真夹具 (行号 0..=256): 前 256 行全夹满，第 257 行是空位行
    let content: String = (0..257).map(|i| format!("l{i}\n")).collect();
    let p = temp_log(content.as_bytes());
    app.file = Arc::new(LogFile::open(&p).unwrap());
    for row in 0..danqing_log::columns::MAX_BOOKMARKS as u64 {
        app.selected = row;
        app.toggle_bookmark();
    }
    assert_eq!(app.bookmarks.len(), danqing_log::columns::MAX_BOOKMARKS);
    // 满员：新增拒绝零变更
    app.selected = 256;
    app.toggle_bookmark();
    assert_eq!(
        app.bookmarks.len(),
        danqing_log::columns::MAX_BOOKMARKS,
        "满员拒绝零变更"
    );
    assert!(
        app.notice
            .as_ref()
            .is_some_and(|(t, k)| t.contains("上限") && matches!(k, crate::NoticeKind::Warn)),
        "拒绝须说清为什么"
    );
    // **删除照常**: 满员时 toggle 已有书签 → 移除成功 (守卫不得堵死腾位路径)
    app.selected = 5;
    app.toggle_bookmark();
    assert!(
        !app.bookmarks.contains(&5),
        "满员时删除已有书签照常 (评审 R②)"
    );
    assert_eq!(app.bookmarks.len(), danqing_log::columns::MAX_BOOKMARKS - 1);
    // 有空位 → 又能加
    app.selected = 256;
    app.toggle_bookmark();
    assert!(app.bookmarks.contains(&256), "有空位即恢复可加");
    std::fs::remove_file(cfg.with_extension("state.json")).ok();
    std::fs::remove_file(&cfg).ok();
    std::fs::remove_file(&p).ok();
}

/// rebuild 越界剔除既有行为回归锁 (T2③另一半): 轮转后 `retain(l < line_count)`。
#[test]
fn apply_rebuild_keeps_bounds_valid_bookmarks() {
    let cfg = temp_cfg_path("bm-rebuild");
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    app.has_file = true;
    app.path = std::path::PathBuf::from("C:\\logs\\rb.log");
    app.bookmarks = [0u64, 5, 99].into_iter().collect();
    let p = temp_log(b"a\nb\n"); // 轮转后 2 行
    let out = OpenOutcome {
        file: LogFile::open(&p).unwrap(),
        schema: None,
        incremental_hits: None,
        rebuilt: true,
        level_column: None,
    };
    app.apply_rebuild(std::path::Path::new("C:\\logs\\rb.log"), out);
    assert_eq!(
        app.bookmarks.iter().copied().collect::<Vec<_>>(),
        vec![0],
        "rebuild 越界剔除"
    );
    std::fs::remove_file(cfg.with_extension("state.json")).ok();
    std::fs::remove_file(&cfg).ok();
    std::fs::remove_file(&p).ok();
}

// ---- 评审修复锁 (2026-09-23 双路评审并账) ----

/// 评审 Critical: 坏 `state.json` 后变更不得**覆盖抹掉全部记忆** ——
/// 先备份 `.bak` (原字节原样), 再开新账。
#[test]
fn corrupt_state_file_is_backed_up_not_clobbered() {
    let cfg = temp_cfg_path("bm-corrupt-save");
    let cols = cfg.with_extension("state.json");
    let bak = cfg.with_extension("state.json.bak");
    // 先造一份 64 条真实记忆，再把文件打成坏 JSON (字节里仍含全部路径)
    let mut files = danqing_log::columns::ColumnFiles::default();
    for i in 0..danqing_log::columns::FILE_CAP {
        files.put(danqing_log::columns::FileEntry {
            path: format!("C:\\logs\\p{i}.log"),
            config: danqing_log::columns::ColumnConfig::default(),
            bookmarks: vec![1],
            updated: i as u64,
        });
    }
    files.save_to(&cols).unwrap();
    let garbage = b"{ this was a good file, now it is not json".to_vec();
    std::fs::write(&cols, &garbage).unwrap();
    // 任意变更触发 save_state
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    app.has_file = true;
    app.path = std::path::PathBuf::from("C:\\logs\\fresh.log");
    let p = temp_log(b"x\ny\n");
    app.file = Arc::new(LogFile::open(&p).unwrap());
    app.selected = 0;
    app.toggle_bookmark();
    assert_eq!(
        std::fs::read(&bak).unwrap(),
        garbage,
        "坏文件原字节进 .bak (记忆可捞回)"
    );
    let after = danqing_log::columns::ColumnFiles::load_from(&cols);
    assert_eq!(after.entries.len(), 1, "新账只含本次条目");
    std::fs::remove_file(&cols).ok();
    std::fs::remove_file(&bak).ok();
    std::fs::remove_file(&cfg).ok();
    std::fs::remove_file(&p).ok();
}

/// 评审 R1: 无有效显示行 (空文件 / 过滤 0 命中 / selected 越界) 不得把
/// 「行 0」幽灵书签落盘 —— 拒绝 + 说清 + 零变更。
#[test]
fn toggle_bookmark_rejects_ghost_rows() {
    let cfg = temp_cfg_path("bm-ghost");
    let cols = cfg.with_extension("state.json");
    // 空文件 (0 行)
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    app.has_file = true;
    app.path = std::path::PathBuf::from("C:\\logs\\empty.log");
    let p = temp_log(b"");
    app.file = Arc::new(LogFile::open(&p).unwrap());
    app.selected = 0;
    app.toggle_bookmark();
    assert!(app.bookmarks.is_empty(), "空文件不夹幽灵行 0");
    // 过滤 0 命中：3 行文件全被滤掉
    let p2 = temp_log(b"a\nb\nc\n");
    app.file = Arc::new(LogFile::open(&p2).unwrap());
    app.filtered = Some(Arc::new(Vec::new()));
    app.selected = 0;
    app.toggle_bookmark();
    assert!(app.bookmarks.is_empty(), "过滤 0 命中不夹行 0");
    // selected 越界 (3 行文件选中第 99 行)
    app.filtered = None;
    app.selected = 99;
    app.toggle_bookmark();
    assert!(app.bookmarks.is_empty(), "越界选中不夹行 0");
    assert!(
        app.notice
            .as_ref()
            .is_some_and(|(t, _)| t.contains("无有效行") || t.contains("无行")),
        "拒绝须说清为什么"
    );
    assert!(!cols.exists(), "零变更不落盘");
    std::fs::remove_file(&cfg).ok();
    std::fs::remove_file(&p).ok();
    std::fs::remove_file(&p2).ok();
}

/// 评审 R1 (五轴 R①): 落盘失败不得谎报「已添加/已去掉」—— 内存照改、
/// 状态说「未落盘」+ 提示。
#[test]
fn toggle_says_truth_when_save_fails() {
    let cfg = temp_cfg_path("bm-savefail");
    let cols = cfg.with_extension("state.json");
    // 开头防御性清理 + 结尾整树删 (见末行注): 本用例历史上会**泄漏**占位目录，
    // 下一次同 pid 复用时 `create_dir` 撞 AlreadyExists → flaky (2026-09-28 抓到)。
    std::fs::remove_dir_all(&cols).ok();
    std::fs::create_dir(&cols).unwrap(); // 非空目录占住落盘路径 → rename 必败
    std::fs::write(cols.join("sentinel"), b"x").unwrap();
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    app.has_file = true;
    app.path = std::path::PathBuf::from("C:\\logs\\sf.log");
    let p = temp_log(b"l0\nl1\n");
    app.file = Arc::new(LogFile::open(&p).unwrap());
    app.selected = 0;
    app.toggle_bookmark();
    assert!(app.bookmarks.contains(&0), "内存照改");
    assert!(app.status.contains("未落盘"), "状态不许说谎");
    assert!(
        app.notice
            .as_ref()
            .is_some_and(|(_, k)| matches!(k, crate::NoticeKind::Warn)),
        "落盘失败须提示"
    );
    // `remove_dir` 删不掉**非空**目录 (里面有 sentinel) → 静默 `.ok()` 把它
    // 永久留在 temp: 等 pid 被复用，下次开头的 create_dir 就撞 AlreadyExists。
    // 用整树删 (flaky 根因，2026-09-28 抓到)。
    std::fs::remove_dir_all(&cols).ok();
    std::fs::remove_file(&cfg).ok();
    std::fs::remove_file(&p).ok();
}

// ---- T1: 拼子句 + picker 状态链 (SPEC-v1x-field-picker-ui D1/D3) ----

/// 拼子句 6 算符 → `parse_query` roundtrip 全等 (语法面零发明);
/// 前缀 = `=` + 值尾 `*` (parse_clause 现语义：值去星收 Prefix)。
#[test]
fn build_clause_six_ops_roundtrip() {
    use danqing_log::jsonl::{self, Clause, Op};
    let cases = [
        ("level", Op::Eq, "ERROR", "level=ERROR"),
        ("level", Op::Prefix, "ERR", "level=ERR*"),
        ("status", Op::GtEq, "500", "status>=500"),
        ("status", Op::LtEq, "499", "status<=499"),
        ("status", Op::Gt, "499", "status>499"),
        ("status", Op::Lt, "500", "status<500"),
    ];
    for (field, op, value, want) in cases {
        let s = build_clause(field, op, value).expect("非空必产出");
        assert_eq!(s, want);
        let q = jsonl::parse_query(&s);
        assert_eq!(q.len(), 1, "{want}");
        match &q[0] {
            Clause::Field {
                path,
                op: got_op,
                value: got_val,
            } => {
                assert_eq!(path.join("."), field);
                assert_eq!(*got_op, op);
                assert_eq!(got_val, value);
            }
            other => panic!("须是 Field 子句：{other:?}"),
        }
    }
    // 点路径字段已被拒收面覆盖 (评审 Critical/R1) —— 语法不支持字面点，
    // `user.id` 走嵌套拆分是 0 命中面，不许当「点路径」放行。
}

/// 空值/空字段/含空白值拒绝 (D3)。
#[test]
fn build_clause_rejects_empty() {
    use danqing_log::jsonl::Op;
    assert!(build_clause("a", Op::Eq, "").is_none(), "空值拒绝");
    assert!(build_clause("", Op::Eq, "v").is_none(), "空字段拒绝");
    assert!(build_clause("a", Op::Eq, "x y").is_none(), "含空白值拒绝");
}

/// 评审 Critical (双路并账): 拼接面必须**盖住** parse 破坏面 ——
/// 被接受的 roundtrip 全等 (path 用 Vec 断言，不许 join 假绿);
/// 会被 `parse_query` 改写语义的一律拒收 (None)。
#[test]
fn build_clause_rejects_anything_parse_would_rewire() {
    use danqing_log::jsonl::{self, Clause, Op};
    // —— 拒收面：值含算符字符 / 算符拼合 / 尾星偷换 / 字段脏字符 ——
    let rejected = [
        ("a", Op::Eq, "List<String>"), // 值含 > <: 切成 a=List Lt String>
        ("a", Op::Eq, "x>y"),
        ("msg", Op::Eq, "a>=b"),
        ("a", Op::Gt, "=b"), // > 与前导 = 拼出 >= 双字符算符
        ("a", Op::Lt, "=1"),
        ("a", Op::Eq, "b*"),       // Eq 尾星被 parse 偷换成 Prefix
        ("a", Op::Prefix, "b*"),   // Prefix 含星双重编码 (b**)
        ("my key", Op::Eq, "v"),   // 字段含空白 → Bare + Field 两子句
        ("user.id", Op::Eq, "42"), // 字段含点 → 扁平键被拆嵌套 (0 命中面)
        ("a>b", Op::Eq, "1"),      // 字段含算符
        ("a=b", Op::Eq, "1"),
        ("a", Op::Eq, "=x"), // 值前导 = 一律拒 (防拼合家族)
    ];
    for (field, op, value) in rejected {
        assert!(
            build_clause(field, op, value).is_none(),
            "须拒收：{field:?} {op:?} {value:?}"
        );
    }
    // —— 接受面：roundtrip 全等 (path 逐段断言) ——
    let accepted = [
        ("level", Op::Eq, "ERROR"),
        ("level", Op::Prefix, "ERR"),
        ("status", Op::GtEq, "500"),
        ("msg", Op::Eq, "a=b"),  // 值内 = 切在第一个，安全
        ("msg", Op::Eq, "100%"), // % 无语义，安全
        ("msg", Op::Eq, "a.b"),  // 值内点不拆 (拆点只在字段侧)
    ];
    for (field, op, value) in accepted {
        let s = build_clause(field, op, value).expect("干净输入必产出");
        let q = jsonl::parse_query(&s);
        assert_eq!(q.len(), 1, "{s}");
        match &q[0] {
            Clause::Field {
                path,
                op: got_op,
                value: got_val,
            } => {
                // path 逐段断言：扁平字段名 = 单段路径 (join 会与点号键假绿)
                assert_eq!(path.as_slice(), [field], "字段须是单段路径：{s}");
                assert_eq!(*got_op, op, "{s}");
                assert_eq!(got_val, value, "{s}");
            }
            other => panic!("须是 Field 子句：{other:?}"),
        }
    }
}

/// PickerSubmit 组装追加 (D3): 无字段提示 / 空查询直提 / 有查询空格连接 AND /
/// 提交即关弹层清草稿 / 空值拒绝不动查询。**摘追加拼接 = 「有查询」断言红 (A/B)**。
#[test]
fn picker_submit_appends_and_applies() {
    let cfg = temp_cfg_path("picker-submit");
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    // 功能已收付费 (gate-trio G2/G3/G4): 本锁验的是付费通路，注付费态;
    // 断言不动 (既有锁不许动语义)。
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    app.has_file = true;
    app.path = std::path::PathBuf::from("C:\\logs\\p.log");
    let p = temp_log(b"{\"level\":\"ERROR\"}\n{\"level\":\"INFO\"}\n");
    app.file = Arc::new(LogFile::open(&p).unwrap());
    // 无字段提交 → 提示且不动
    app.update(Msg::PickerSubmit("ERROR".into()));
    assert!(app.notice.is_some(), "无字段说清");
    assert!(app.filter_applied.is_empty());
    // 选字段，空查询直提 (D3: 就是它)
    app.update(Msg::OpenPicker);
    app.update(Msg::PickPickerField("level".into()));
    app.update(Msg::PickerSubmit("ERROR".into()));
    assert_eq!(app.filter_applied, "level=ERROR", "空查询直提");
    assert!(!app.picker_open, "提交即关弹层");
    assert!(app.picker_field.is_none(), "草稿清");
    // 再开 + 算符：追加 AND (空格连接)
    app.update(Msg::OpenPicker);
    app.update(Msg::PickPickerField("level".into()));
    app.update(Msg::PickPickerOp(jsonl::Op::Prefix));
    app.update(Msg::PickerSubmit("ER".into()));
    assert_eq!(
        app.filter_applied, "level=ERROR level=ER*",
        "有查询 = 空格连接 AND"
    );
    // 空值拒绝：查询不动，弹层不关 (留着补值)
    app.update(Msg::OpenPicker);
    app.update(Msg::PickPickerField("level".into()));
    app.update(Msg::PickerSubmit("".into()));
    assert_eq!(app.filter_applied, "level=ERROR level=ER*", "空值不动查询");
    assert!(app.picker_open, "拒绝不关弹层");
    std::fs::remove_file(cfg.with_extension("state.json")).ok();
    std::fs::remove_file(&cfg).ok();
    std::fs::remove_file(&p).ok();
}

/// 互斥 (D1): 开一关二 —— picker 与 col_menu/export_menu 双向。
#[test]
fn picker_open_is_mutually_exclusive() {
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("picker-mutex")));
    // 功能已收付费 (gate-trio G2/G3/G4): 本锁验的是付费通路，注付费态;
    // 断言不动 (既有锁不许动语义)。
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    app.col_menu_open = true;
    app.export_menu_open = true;
    app.update(Msg::OpenPicker);
    assert!(app.picker_open && !app.col_menu_open && !app.export_menu_open);
    app.col_menu_open = false;
    app.export_menu_open = false;
    app.update(Msg::OpenColMenu);
    assert!(app.col_menu_open && !app.picker_open, "开列管理关 picker");
    app.picker_open = false;
    // export 打开路径 (入口函数) 同款 —— 直接看臂上互斥
    app.picker_open = true;
    app.update(Msg::OpenPicker);
    assert!(app.picker_open);
    std::fs::remove_file(temp_cfg_path("picker-mutex")).ok();
}

/// T2 接线锁族 (评审修复后语义): Esc 次序 / 全局 Enter **不**劫 (R7) /
/// 模态守卫 / 关弹层清草稿 (reset_picker_draft)。
#[test]
fn picker_esc_enter_modal_and_draft_clear() {
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("picker-wire")));
    let esc = Event::Key {
        key: Key::Named(NamedKey::Escape),
        pressed: true,
        shift: false,
        ctrl: false,
        alt: false,
    };
    let enter = Event::Key {
        key: Key::Named(NamedKey::Enter),
        pressed: true,
        shift: false,
        ctrl: false,
        alt: false,
    };
    // Esc 次序：picker 先于 col_menu (D1 插层)
    app.picker_open = true;
    app.col_menu_open = true;
    assert!(matches!(app.app_key_filter(&esc), Some(Msg::ClosePicker)));
    // 评审 R7: 全局 Enter **不**拦截 (算符钮的 Enter 归按钮激活);
    // 提交归 PickerInput 持有者内 (其单元锁见 settings 侧)
    assert!(app.app_key_filter(&enter).is_none());
    // 模态守卫：picker 开时全局 Ctrl 键不穿 (Ctrl+L 探针，不碰 Ctrl+O)
    let ctrl_l = Event::Key {
        key: Key::Character("l".to_string()),
        pressed: true,
        shift: false,
        ctrl: true,
        alt: false,
    };
    app.picker_open = true;
    assert!(matches!(app.app_key_filter(&ctrl_l), Some(Msg::Noop)));
    // 剪辑键放行 (许可页粘贴先例)
    let ctrl_v = Event::Key {
        key: Key::Character("v".to_string()),
        pressed: true,
        shift: false,
        ctrl: true,
        alt: false,
    };
    assert!(app.app_key_filter(&ctrl_v).is_none());
    // 关弹层清草稿三态 (reset_picker_draft 一处收口)
    app.picker_field = Some("x".into());
    app.picker_op = jsonl::Op::Gt;
    let rev = app.picker_clear_rev;
    app.update(Msg::ClosePicker);
    assert!(!app.picker_open);
    assert!(app.picker_field.is_none(), "草稿清");
    assert_eq!(app.picker_op, jsonl::Op::Eq, "算符复位默认");
    assert!(app.picker_clear_rev > rev, "清空代次前进 (输入框清)");
    std::fs::remove_file(temp_cfg_path("picker-wire")).ok();
}

/// 评审 R5: `OpenSettings` 与弹层族互斥 (托盘无模态屏障) ——
/// 双开会让 Enter 被劫到提交; 开设置先关尽。
#[test]
fn open_settings_closes_transient_popovers() {
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("picker-r5")));
    app.picker_open = true;
    app.col_menu_open = true;
    app.export_menu_open = true;
    app.update(Msg::OpenSettings);
    assert!(app.settings_open);
    assert!(
        !app.picker_open && !app.col_menu_open && !app.export_menu_open,
        "开设置关弹层族"
    );
    // UpgradeGotoActivate 同规
    app.picker_open = true;
    app.update(Msg::UpgradeGotoActivate);
    assert!(app.settings_open && !app.picker_open);
    std::fs::remove_file(temp_cfg_path("picker-r5")).ok();
}

/// 评审 R6: 弹层开着时无人认领的导航键/滚轮不许穿到弹层后 (09-14 同族)。
#[test]
fn modal_gate_swallows_nav_keys_and_wheel_when_picker_open() {
    let cfg = temp_cfg_path("picker-r6");
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    app.has_file = true;
    let p = temp_log(
        (0..20)
            .map(|i| format!("l{i}\n"))
            .collect::<String>()
            .as_bytes(),
    );
    app.file = Arc::new(LogFile::open(&p).unwrap());
    app.picker_open = true;
    // ↓ 无人认领 → 不得滚日志
    app.event(&Event::Key {
        key: Key::Named(NamedKey::ArrowDown),
        pressed: true,
        shift: false,
        ctrl: false,
        alt: false,
    });
    assert_eq!(app.top_row, 0.0, "picker 开着 ↓ 不穿到日志");
    // 滚轮同锁 (评审 FYI 顺手补)
    app.event(&Event::MouseWheel {
        delta: (0.0, 3.0),
        position: danqing::Point::new(10.0, 10.0),
        shift: false,
        ctrl: false,
        alt: false,
    });
    assert_eq!(app.top_row, 0.0, "picker 开着滚轮不穿到日志");
    // 对照：关掉后照常滚
    app.picker_open = false;
    app.event(&Event::Key {
        key: Key::Named(NamedKey::ArrowDown),
        pressed: true,
        shift: false,
        ctrl: false,
        alt: false,
    });
    assert!(app.top_row > 0.0, "无弹层时 ↓ 照常滚");
    std::fs::remove_file(cfg).ok();
    std::fs::remove_file(&p).ok();
}

#[test]
fn app_starts_free_in_test_mode() {
    let app = LogApp::new_empty_at(Some(temp_cfg_path("free")));
    assert_eq!(app.entitlement, Entitlement::Free);
}

#[test]
fn activate_license_flips_entitlement_and_persists() {
    let cfg = temp_cfg_path("activate");
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    let (key, pk) = test_sign(LICENSE_PAYLOAD);
    app.license_pubkey = pk;
    app.activate_license(key);
    assert!(matches!(app.entitlement, Entitlement::Paid { .. }));
    // 落盘在注入路径的同名邻居，且重启 (重新 load) 能验回 = 持久化语义
    let lic = cfg.with_extension("license.key");
    let loaded = license::load_from(&lic, &pk);
    assert!(matches!(loaded, Entitlement::Paid { .. }));
    std::fs::remove_file(&lic).ok();
    std::fs::remove_file(&cfg).ok();
}

#[test]
fn activate_license_bad_key_stays_free_and_writes_nothing() {
    let cfg = temp_cfg_path("badkey");
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    let (mut key, pk) = test_sign(LICENSE_PAYLOAD);
    key.push('x'); // 破坏尾段
    app.license_pubkey = pk;
    app.activate_license(key);
    assert_eq!(app.entitlement, Entitlement::Free);
    assert!(
        !cfg.with_extension("license.key").exists(),
        "坏 key 不许落盘"
    );
}

// ─── v1x-licensing T5: 商店半边状态采纳 ───

#[test]
fn store_query_only_upgrades_from_free_never_clobbers() {
    // D4: 会话内已激活的授权不被晚到的商店查询覆盖 (会话内不踢人)。
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("storeadopt")));
    app.adopt_store_entitlement(Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    });
    assert!(matches!(app.entitlement, Entitlement::Paid { .. }));
    // 已 Paid 后，商店侧再回 Free (例如查询失败 fail-open) 不许把人踢下来
    app.adopt_store_entitlement(Entitlement::Free);
    assert!(matches!(app.entitlement, Entitlement::Paid { .. }));
}

#[test]
fn purchase_outcome_purchased_unlocks_failed_stays_free() {
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("purchase")));
    app.adopt_purchase_outcome(store_license::PurchaseOutcome::Purchased);
    assert!(matches!(
        app.entitlement,
        Entitlement::Paid {
            source: PaidSource::StoreAddOn
        }
    ));
    let mut app2 = LogApp::new_empty_at(Some(temp_cfg_path("purchase2")));
    app2.adopt_purchase_outcome(store_license::PurchaseOutcome::Failed);
    assert_eq!(app2.entitlement, Entitlement::Free);
    app2.adopt_purchase_outcome(store_license::PurchaseOutcome::Cancelled);
    assert_eq!(app2.entitlement, Entitlement::Free);
}

// ─── v1x-field-analytics T4: 门控与发起 ───

#[test]
fn analyze_field_gated_for_free_and_launches_for_paid() {
    let cfg = temp_cfg_path("fagate");
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    app.has_file = true;
    app.schema = Some(Arc::new(Schema {
        columns: vec![jsonl::Column {
            name: "d".into(),
            width_chars: 1,
        }],
    }));
    // 免费态：弹升级提示，不发起扫描
    app.update(Msg::AnalyzeField(0));
    assert_eq!(app.upgrade_prompt, Some(Feature::FieldAnalytics));
    assert_eq!(app.analysis_launches, 0, "免费态不得发起扫描");
    assert!(!app.analysis_running);
    // 付费态：发起并拾取
    let (key, pk) = test_sign(LICENSE_PAYLOAD);
    app.license_pubkey = pk;
    app.activate_license(key);
    app.update(Msg::CloseUpgradePrompt);
    app.update(Msg::AnalyzeField(0));
    assert_eq!(app.analysis_launches, 1);
    assert!(app.analysis_running);
    assert_eq!(app.upgrade_prompt, None);
    // job 完成拾取 (空文件秒回; 自旋上限 2s 防 flake)
    let mut picked = false;
    for _ in 0..200 {
        if let Some(a) = app.analysis_job.poll() {
            app.analysis_running = false;
            app.analysis_result = Some(a);
            picked = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(picked, "分析作业应完成");
    assert_eq!(
        app.analysis_result.as_ref().unwrap().scope_rows,
        0,
        "空文件 0 行"
    );
    std::fs::remove_file(cfg.with_extension("license.key")).ok();
}

/// 评审 R5: 过滤作业在途窗口内，`filter_applied` 是新串而 `filtered`
/// 行集还是旧的 —— 分析快照必须读**落账串** (与行集同源), 否则结果
/// 看起来新鲜、数字其实是上一个过滤的 (stale 永 false 的静默错数)。
#[test]
fn analyze_snapshots_landed_filter_not_pending_one() {
    let cfg = temp_cfg_path("fawin");
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    app.has_file = true;
    app.schema = Some(Arc::new(Schema {
        columns: vec![jsonl::Column {
            name: "d".into(),
            width_chars: 1,
        }],
    }));
    let (key, pk) = test_sign(LICENSE_PAYLOAD);
    app.license_pubkey = pk;
    app.activate_license(key);
    // 过滤 A 已落账
    app.filtered = Some(Arc::new(vec![0, 2]));
    app.filter_applied = "level=ERROR".to_string();
    app.filter_landed = "level=ERROR".to_string();
    // 发起过滤 B → 在途窗口：applied=B, 行集与 landed 仍是 A
    app.apply_filter("status=500".to_string());
    assert!(app.filter_pending);
    assert_eq!(app.filter_applied, "status=500");
    // 窗口内发起分析：快照取落账串 A (与行集同源)
    app.analyze_field(0);
    assert_eq!(
        app.analysis_filter_src, "level=ERROR",
        "分析快照必须与行集同源 (落账串), 不能读在途的新串"
    );
    // B 落账：landed 换串 —— 之后面板的 stale 判据 (src != applied) 仍成立
    let mut landed = false;
    for _ in 0..200 {
        if let Some(out) = app.filter_job.poll() {
            app.filter_pending = false;
            app.filter_landed = app.filter_applied.clone();
            app.filtered = Some(Arc::new(out.lines));
            landed = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(landed, "过滤作业应完成");
    assert_eq!(app.filter_landed, "status=500");
    std::fs::remove_file(cfg.with_extension("license.key")).ok();
}

/// R5 同族：Esc 清过滤 / 空查询回全量，都得作废**在途**的过滤作业 ——
/// 否则它晚到把行集贴回来，底栏显示「无过滤」而列表是过滤后的。
#[test]
fn clear_filter_kills_pending_filter_job() {
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("faclr")));
    app.has_file = true;
    app.apply_filter("level=ERROR".to_string());
    assert!(app.filter_pending);
    app.clear_filter();
    assert!(!app.filter_pending);
    assert!(app.filtered.is_none());
    assert!(app.filter_landed.is_empty());
    // 在途作业的结果晚到也不得复活
    std::thread::sleep(std::time::Duration::from_millis(100));
    let mut resurrected = false;
    for _ in 0..50 {
        if app.filter_job.poll().is_some() {
            resurrected = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(!resurrected, "invalidate 后在途结果必须被丢弃，poll 不出");
    assert!(app.filtered.is_none());
}

#[test]
fn upgrade_prompt_shows_only_when_not_entitled() {
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("upg")));
    app.update(Msg::ShowUpgradePrompt(Feature::Export));
    assert_eq!(app.upgrade_prompt, Some(Feature::Export));
    // 付费态不出现 (两道闸里的第二道): 激活后**先清掉上一个提示**再发，
    // 提示必须不再出现
    let (key, pk) = test_sign(LICENSE_PAYLOAD);
    app.license_pubkey = pk;
    app.activate_license(key);
    app.update(Msg::CloseUpgradePrompt);
    app.update(Msg::ShowUpgradePrompt(Feature::Export));
    assert_eq!(app.upgrade_prompt, None, "付费态不许出现升级提示");
    std::fs::remove_file(temp_cfg_path("upg").with_extension("license.key")).ok();
}

#[test]
fn upgrade_goto_activate_opens_license_tab() {
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("upggoto")));
    app.update(Msg::ShowUpgradePrompt(Feature::FieldAnalytics));
    app.update(Msg::UpgradeGotoActivate);
    assert_eq!(app.upgrade_prompt, None, "跳转后提示要关");
    assert!(app.settings_open, "跳转后设置卡要开");
    assert_eq!(
        app.settings_tab,
        settings::LICENSE_TAB_INDEX,
        "停在「许可」页"
    );
}

#[test]
fn esc_closes_upgrade_prompt_before_settings() {
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("upgesc")));
    app.settings_open = true;
    app.upgrade_prompt = Some(Feature::Export);
    let esc = Event::Key {
        key: Key::Named(NamedKey::Escape),
        pressed: true,
        shift: false,
        ctrl: false,
        alt: false,
    };
    let msg = app.app_key_filter(&esc);
    assert!(
        matches!(msg, Some(Msg::CloseUpgradePrompt)),
        "升级提示在最上层时 Esc 先关它"
    );
}

// ─── SPEC-v1x-export T5 (导出全链路 UI) ───

/// D6 判据：免费态点导出 = 弹升级提示，**不开格式菜单** (保存对话框之前拦)。
/// 「零落盘」由构造保证：该分支根本不走到 `launch_export`。
#[test]
fn export_entry_free_tier_prompts_upgrade_without_menu() {
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("expgate")));
    app.has_file = true;
    app.update(Msg::ExportEntryClicked);
    assert_eq!(app.upgrade_prompt, Some(Feature::Export));
    assert!(!app.export_menu_open, "免费态不得开格式菜单");
}

/// 付费态开格式菜单; 作业态下同一入口 = 取消 (D2 单作业语义), 不再开菜单。
#[test]
fn export_entry_paid_opens_menu_and_running_entry_cancels() {
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("exppaid")));
    app.has_file = true;
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    app.update(Msg::ExportEntryClicked);
    assert!(app.export_menu_open, "付费态开格式菜单");
    assert_eq!(app.upgrade_prompt, None);
    // 作业态 (真作业压 running): 入口点 = 取消，不是再开菜单
    let p = temp_log(b"a\nb\n");
    let out = OpenOutcome {
        file: LogFile::open(&p).unwrap(),
        schema: None,
        incremental_hits: None,
        rebuilt: false,
        level_column: None,
    };
    app.apply_fresh(p.clone(), out);
    let dest = std::env::temp_dir().join(format!("dq-exp-cancel-{}.log", std::process::id()));
    app.launch_export(dest.clone(), ExportPick::Raw);
    assert!(app.export_job.is_running());
    app.export_menu_open = false; // 上一段菜单已用完，复位再测作业态
    app.update(Msg::ExportEntryClicked);
    assert!(!app.export_menu_open, "作业态点入口 = 取消，不开菜单");
    // 收尾拾取 (取消或没赶上都是自洽收尾; 半成品语义由 export 单测锁)
    for _ in 0..200 {
        if app.export_job.poll().is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    std::fs::remove_file(&p).ok();
    std::fs::remove_file(&dest).ok();
}

/// `Ctrl+E` 与底栏按钮同消息; 格式菜单开着时 = 模态，不得穿透 (T16 同纪律)。
#[test]
fn ctrl_e_dispatches_export_entry_and_menu_modal_eats_it() {
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("expkey")));
    let mk = |ch: &str| Event::Key {
        key: Key::Character(ch.to_string()),
        pressed: true,
        shift: false,
        ctrl: true,
        alt: false,
    };
    assert!(
        matches!(app.app_key_filter(&mk("e")), Some(Msg::ExportEntryClicked)),
        "Ctrl+E 必须发导出入口消息"
    );
    app.export_menu_open = true;
    assert!(
        matches!(app.app_key_filter(&mk("e")), Some(Msg::Noop)),
        "格式菜单开着 = 模态，Ctrl+E 不穿透"
    );
}

/// Esc 次序：升级提示 > 设置卡 > 导出格式菜单 > 栏。
#[test]
fn esc_closes_export_menu_after_higher_modals() {
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("expesc")));
    let esc = Event::Key {
        key: Key::Named(NamedKey::Escape),
        pressed: true,
        shift: false,
        ctrl: false,
        alt: false,
    };
    app.export_menu_open = true;
    app.settings_open = true;
    assert!(
        matches!(app.app_key_filter(&esc), Some(Msg::CloseSettings)),
        "设置卡在导出菜单之上"
    );
    app.settings_open = false;
    assert!(
        matches!(app.app_key_filter(&esc), Some(Msg::CloseExportMenu)),
        "设置卡关后 Esc 关导出菜单"
    );
}

/// 明文服务端守门：菜单收口是 UI 层，点到了也不放行 (测试不碰真对话框 ——
/// 该分支在弹框**之前**返回，故可测)。
#[test]
fn begin_export_rejects_pretty_csv_on_plain_file() {
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("expplain")));
    let p = temp_log(b"plain line\n");
    let out = OpenOutcome {
        file: LogFile::open(&p).unwrap(),
        schema: None,
        incremental_hits: None,
        rebuilt: false,
        level_column: None,
    };
    app.apply_fresh(p.clone(), out);
    app.export_menu_open = true;
    app.begin_export(ExportPick::Pretty);
    assert!(!app.export_job.is_running(), "非 JSONL 不得发起美化导出");
    assert!(!app.export_menu_open, "菜单照常收起");
    app.export_menu_open = true;
    app.begin_export(ExportPick::Csv);
    assert!(!app.export_job.is_running(), "非 JSONL 不得发起 CSV 导出");
    std::fs::remove_file(&p).ok();
}

/// 全链路 (D1 行集口径): JSONL + 过滤命中 → launch_export 写出恰是那两行。
/// 走 `launch_export` 而非 `begin_export` —— 后者会弹真保存对话框
/// (测试严禁真实桌面副作用)。
#[test]
fn launch_export_writes_filtered_line_set_end_to_end() {
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("expe2e")));
    let p = temp_log(b"{\"level\":\"ERROR\"}\n{\"level\":\"INFO\"}\n{\"level\":\"ERROR\"}\n");
    let f = LogFile::open(&p).unwrap();
    let out = OpenOutcome {
        schema: jsonl::discover_schema(&f),
        file: f,
        incremental_hits: None,
        rebuilt: false,
        level_column: Some("level".into()),
    };
    app.apply_fresh(p.clone(), out);
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    app.filtered = Some(Arc::new(vec![0, 2]));
    let dest = std::env::temp_dir().join(format!("dq-exp-e2e-{}.log", std::process::id()));
    app.launch_export(dest.clone(), ExportPick::Raw);
    let mut result = None;
    for _ in 0..200 {
        if let Some(r) = app.export_job.poll() {
            result = Some(r);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let r = result.expect("导出作业应完成");
    assert!(matches!(r.end, ExportEnd::Done { lines: 2, .. }));
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        b"{\"level\":\"ERROR\"}\n{\"level\":\"ERROR\"}\n",
        "行集 = 过滤命中 (0, 2), 字节保真"
    );
    std::fs::remove_file(&p).ok();
    std::fs::remove_file(&dest).ok();
}

/// 成功判据 4 (review R2): D1 行集口径表**四态逐格上锁** —— 修前只测了
/// 「JSONL+过滤」一格，分支写反/冻结失效仍全绿。
#[test]
fn export_line_set_covers_d1_four_states() {
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("d1a")));
    // 真文件 (行集要 clamp 到 line_count —— 空 LogApp 的 0 行会把命中全夹没)
    let p = temp_log(b"0\n1\n2\n3\n4\n5\n");
    app.file = Arc::new(LogFile::open(&p).unwrap());
    // ① JSONL + 过滤生效 → 过滤命中集
    app.schema = Some(Arc::new(jsonl::Schema {
        columns: vec![jsonl::Column {
            name: "a".into(),
            width_chars: 1,
        }],
    }));
    app.filtered = Some(Arc::new(vec![3, 1]));
    let set = app.export_line_set();
    assert!(!set.is_full(), "① JSONL+过滤 = 稀疏命中");
    assert_eq!(set.lines(), &[1, 3], "① 升序去重");

    // ② JSONL 无过滤 → 全集 (搜索**不改**行集 —— 即便搜索在途)
    app.filtered = None;
    app.search = Some(SearchNav::new(Arc::new(vec![0]), 1));
    let set = app.export_line_set();
    assert!(set.is_full(), "② JSONL 无过滤 = 全集，搜索不改口径");

    // ③ 明文 + 搜索生效 → 含命中的行
    app.schema = None;
    app.search = Some(SearchNav::new(Arc::new(vec![5, 2]), 2));
    let set = app.export_line_set();
    assert!(!set.is_full(), "③ 明文 + 搜索 = 命中行集");
    assert_eq!(set.lines(), &[2, 5]);

    // ④ 明文 无搜索 → 全集
    app.search = None;
    let set = app.export_line_set();
    assert!(set.is_full(), "④ 明文无搜索 = 全集");
    std::fs::remove_file(&p).ok();
}

/// review R1 回归锁：默认扩展名按格式分派 (原始行保源 / 美化 .json / CSV .csv) ——
/// 修前恒取源扩展名，`server.jsonl` 导 CSV 默认名还是 `.jsonl`。
#[test]
fn default_export_ext_follows_format() {
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("d8ext")));
    app.path = std::path::PathBuf::from("server.jsonl");
    assert_eq!(
        app.export_name_parts(ExportPick::Raw).unwrap().2,
        "jsonl",
        "原始行保源扩展名"
    );
    assert_eq!(
        app.export_name_parts(ExportPick::Pretty).unwrap().2,
        "json",
        "美化 = .json"
    );
    assert_eq!(
        app.export_name_parts(ExportPick::Csv).unwrap().2,
        "csv",
        "CSV = .csv"
    );
}

/// review B-R1 回归锁：搜索命中被导航表封顶 (100 万) 时**拒绝导出** ——
/// 静默截断交付物是对账事故 (修前：导出前 100 万行且报「完成 N 行」)。
/// 走**入口消息** (闸在入口，D6 同点位) —— 别直接调 begin_export:
/// 那会穿到真保存对话框 (测试严禁真实桌面副作用; 本测试第一版就踩了这个)。
#[test]
fn capped_search_hits_refuse_export() {
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("cap")));
    let p = temp_log(b"a\nb\n");
    let out = OpenOutcome {
        file: LogFile::open(&p).unwrap(),
        schema: None,
        incremental_hits: None,
        rebuilt: false,
        level_column: None,
    };
    app.apply_fresh(p.clone(), out);
    // total=200 万 > hits 表 2 条 = 被封顶的形态
    app.search = Some(SearchNav::new(Arc::new(vec![0, 1]), 2_000_000));
    app.update(Msg::ExportEntryClicked);
    assert!(!app.export_job.is_running(), "封顶命中不得静默截断导出");
    assert!(
        app.notice
            .as_ref()
            .is_some_and(|(t, _)| t.contains("收窄搜索")),
        "必须明说原因"
    );
    std::fs::remove_file(&p).ok();
}

/// review R4 回归锁：换文件 (apply_fresh) 必须关掉格式菜单 ——
/// 修前菜单残留，落地后点格式导出的是**新**文件 (旧文件语境的菜单)。
#[test]
fn apply_fresh_closes_export_menu() {
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("freshmenu")));
    app.export_menu_open = true;
    let p = temp_log(b"x\n");
    let out = OpenOutcome {
        file: LogFile::open(&p).unwrap(),
        schema: None,
        incremental_hits: None,
        rebuilt: false,
        level_column: None,
    };
    app.apply_fresh(p.clone(), out);
    assert!(!app.export_menu_open, "换文件必须作废格式菜单");
    std::fs::remove_file(&p).ok();
}

/// review Optional: 过滤在途闸 —— Enter 后立刻导出不得静默拿到上一份行集。
#[test]
fn pending_filter_blocks_export_entry() {
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("pending")));
    app.has_file = true;
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    app.filter_pending = true;
    app.update(Msg::ExportEntryClicked);
    assert!(
        !app.export_menu_open,
        "过滤计算中不开菜单 (开了就会静默导出上一份行集)"
    );
}

// ─── 评审修复 (2026-09-19) ───

/// 评审 Critical 回归锁：模态卡开着时 Ctrl+V 必须放行给焦点分发
/// (否则许可页输入框没法粘贴 key = 激活主路径断裂); 其它全局键仍拦。
#[test]
fn modal_card_passes_clipboard_keys_but_blocks_globals() {
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("clipboard")));
    app.settings_open = true;
    let mk = |ch: &str| Event::Key {
        key: Key::Character(ch.to_string()),
        pressed: true,
        shift: false,
        ctrl: true,
        alt: false,
    };
    for combo in ["v", "c", "x", "a", "z", "y"] {
        assert!(
            app.app_key_filter(&mk(combo)).is_none(),
            "Ctrl+{combo} 必须放行 (焦点分发里的剪贴板路由)"
        );
    }
    // 大写 (Shift 态) 同样放行
    assert!(app.app_key_filter(&mk("V")).is_none());
    // 非剪辑全局键仍被拦 (Ctrl+F 不得穿透到卡后)
    assert!(matches!(app.app_key_filter(&mk("f")), Some(Msg::Noop)));
}

/// 评审 Required: 购买防重入闸 —— 在途拒绝二次发起，结果回来复位。
#[test]
fn purchase_in_flight_gate_blocks_reentry_and_resets() {
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("reentry")));
    assert!(app.try_begin_purchase(), "空闲时应放行");
    assert!(!app.try_begin_purchase(), "在途时应拦住");
    app.adopt_purchase_outcome(store_license::PurchaseOutcome::Cancelled);
    assert!(!app.purchase_in_flight, "结果回来要复位");
    assert!(app.try_begin_purchase(), "复位后又能发起");
}

/// 评审 Required: 购买结果反馈落卡内 (`license_feedback`), 不走被模态
/// 遮住的底栏 notice。
#[test]
fn purchase_outcome_feedback_lands_in_card() {
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("pfb")));
    app.adopt_purchase_outcome(store_license::PurchaseOutcome::Purchased);
    assert!(app.license_feedback.is_some(), "成功反馈要在卡内可见");
    assert!(matches!(app.entitlement, Entitlement::Paid { .. }));
}

/// 评审 Optional: 已是付费层时「获取付费层」不再进购买流程。
#[test]
fn purchase_when_already_paid_only_answers_back() {
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("paidagain")));
    let (key, pk) = test_sign(LICENSE_PAYLOAD);
    app.license_pubkey = pk;
    app.activate_license(key);
    assert!(matches!(app.entitlement, Entitlement::Paid { .. }));
    app.purchase_paid_layer();
    assert!(
        matches!(&app.license_feedback, Some((t, _)) if t.contains("无需重复购买")),
        "已购再点要给明确回话：{:?}",
        app.license_feedback
    );
    assert!(!app.purchase_in_flight, "已购不得发起购买");
    std::fs::remove_file(temp_cfg_path("paidagain").with_extension("license.key")).ok();
}

/// 安全评审：激活成功后输入镜像清空 + rev  bumped (widget 侧 bind_clear
/// 会把框内明文一并清掉)。
#[test]
fn successful_activation_clears_key_input() {
    let cfg = temp_cfg_path("clearonok");
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    let (key, pk) = test_sign(LICENSE_PAYLOAD);
    app.license_pubkey = pk;
    app.license_key_input = key.clone();
    let rev0 = app.license_clear_rev;
    app.activate_license(key);
    assert!(app.license_key_input.is_empty(), "镜像要清");
    assert_eq!(app.license_clear_rev, rev0 + 1, "清空代次要涨");
    std::fs::remove_file(cfg.with_extension("license.key")).ok();
}

/// expand_rev (M3/T6): 实际展开/折叠才 +1; parse 失败/无嵌套不涨 ——
/// 守卫的触发源必须精确，虚涨会误杀活着的选区 (LogView 侧见
/// `sync_clears_selection_when_expand_rev_changes`)。
#[test]
fn toggle_expand_bumps_expand_rev_only_on_real_change() {
    let mut app = LogApp::new_empty();
    let p = temp_log("{\"a\":{\"b\":1}}\nplain\n".as_bytes());
    app.file = Arc::new(LogFile::open(&p).unwrap());
    let rev0 = app.expand_rev;
    app.toggle_expand(0); // 展开含嵌套行
    assert_eq!(app.expand_rev, rev0 + 1);
    app.toggle_expand(1); // 明文行 parse 失败 → 不涨
    assert_eq!(app.expand_rev, rev0 + 1);
    app.toggle_expand(0); // 折叠
    assert_eq!(app.expand_rev, rev0 + 2);
    std::fs::remove_file(&p).ok();
}

#[test]
fn clamp_top_bounds() {
    assert_eq!(clamp_top(-5.0, 100), 0.0, "负数钳到 0");
    assert_eq!(clamp_top(500.0, 100), 99.0, "越界钳到末行");
    assert_eq!(clamp_top(42.5, 100), 42.5, "区间内不变 (保小数偏移)");
    assert_eq!(clamp_top(3.0, 0), 0.0, "空文件归零");
}

/// `title_theme` 的六项必须**取自 `LogTheme`**, 不再手抄。
///
/// 手抄的后果是同一个界面里出现**两套强调色**: 浅色分支的 accent 曾经是蓝
/// `0.18,0.35,0.60`, 而框架玉色是 `#0F766E`。六个值全部改成从 `LogTheme` 取，
/// 只剩 `backdrop_light/dark` 手写 (框架没有对应 token, 它们只服务标题栏
/// 这一层场景)。
#[test]
fn title_theme_derives_tokens_from_log_theme() {
    for app in [config::AppTheme::Light, config::AppTheme::Dark] {
        let scene = title_theme(app);
        let t = app.theme();
        assert_eq!(scene.background(), t.background(), "{app:?} base");
        assert_eq!(scene.accent(), t.accent(), "{app:?} accent");
        assert_eq!(
            scene.text_primary(),
            t.text_primary(),
            "{app:?} text_primary"
        );
        assert_eq!(
            scene.text_secondary(),
            t.text_secondary(),
            "{app:?} text_secondary"
        );
        assert_eq!(scene.surface(), t.surface(), "{app:?} surface");
        assert_eq!(
            scene.surface_input(),
            t.surface_input(),
            "{app:?} surface_input"
        );
    }
}

/// 切主题后**标题栏文字色必须跟着变** —— 它靠 `bind_theme` 每帧重取，
/// 不能靠构造。
///
/// 复现的是这个缺陷：`view()` 只在启动时求值一次
/// (`danqing/src/window/mod.rs:207` 的 `let tree = app.view();`),
/// 所以构造时烘进 `TitleBar` 的主题色**不随切换而变**, 而底色 (清屏色) 变了
/// → 切到浅色是浅字压浅底、切到暗色是暗字压暗底。用户实机报的两个方向都成立。
///
/// **关键在「构造用一个主题、sync 用另一个」**: 两边都用同一主题的话，
/// 就算 `bind_theme` 掉了也照样绿 —— 那样测的是构造，不是绑定。
#[test]
fn title_bar_colors_follow_theme_switch() {
    use danqing::{Constraints, Point, Rect, RectBatch, TextBatch};

    // 构造用暗色 —— 字号/间距两主题相同, 变的只有颜色。
    let mut dark_app = LogApp::new_empty();
    dark_app.theme = config::AppTheme::Dark;
    let mut bar = title_bar(dark_app.theme, dark_app.make_title());

    // 每帧状态换成浅色 (模拟运行中切主题)。
    let mut light_app = LogApp::new_empty();
    light_app.theme = config::AppTheme::Light;
    bar.sync(&light_app);

    let mut texts = TextBatch::default();
    let mut rects = RectBatch::new();
    let size = bar.layout(Constraints::tight(Size::new(1100.0, 40.0)), &mut texts);
    bar.paint(Rect::new(Point::ZERO, size), &mut rects, &mut texts);

    // 期望值**从主题取**, 不写字面量：原先这里钉的是手抄时代的 `0.12`,
    // R5 把 `title_theme` 改成取自 `LogTheme` 后它就过期了 (token 是 #0F172A),
    // 断言随即变红 —— 那条红是真的，说明它确实盯着颜色。
    let t = danqing::theme::LightTheme.text_primary();
    let want = danqing::srgb_to_linear(t.r);
    let hit = texts
        .instance_colors()
        .iter()
        .any(|c| (c.r - want).abs() < 1e-3);
    assert!(
        hit,
        "切到浅色后标题文字应变成浅色主题的正文色 {t:?} —— 没命中即 `bind_theme` 缺失"
    );
}

#[test]
fn window_clear_color_follows_theme() {
    // 清屏色的**单点定义** (AD1): 启动与切主题都取它，不许两处各算一份。
    // 回归的是这个缺陷：清屏色原本写死浅色，且全仓**零处** set_clear_color 调用 ——
    // 于是存暗色配置启动、或运行中切到暗色，窗口底色纹丝不动。
    // 标题栏那条亮带**就是**清屏色 (框架 TitleBar 背景是有意的 TRANSPARENT)。
    use danqing::theme::{DarkTheme, LightTheme, Theme};
    assert_eq!(
        window_clear_color(config::AppTheme::Light),
        LightTheme.background(),
        "浅色：清屏色 = 主题背景"
    );
    assert_eq!(
        window_clear_color(config::AppTheme::Dark),
        DarkTheme.background(),
        "暗色：清屏色 = 主题背景"
    );
    assert_ne!(
        window_clear_color(config::AppTheme::Light),
        window_clear_color(config::AppTheme::Dark),
        "两主题的清屏色必须不同 —— 相同即等于没跟随 (本缺陷的原始形态)"
    );
}

#[test]
fn next_bookmark_strictly_after_and_wraps() {
    let mut set = std::collections::BTreeSet::new();
    assert_eq!(next_bookmark(&set, 0), None, "空集 None");
    set.extend([3, 10, 20]);
    assert_eq!(next_bookmark(&set, 0), Some(3));
    assert_eq!(next_bookmark(&set, 3), Some(10), "严格大于 (不跳自身)");
    assert_eq!(next_bookmark(&set, 20), Some(3), "末尾环绕");
    assert_eq!(next_bookmark(&set, u64::MAX), Some(3), "MAX 不溢出");
    assert_eq!(next_bookmark(&set, 15), Some(20));
}

#[test]
fn build_search_pattern_utf8_keeps_regex_gbk_literalizes() {
    // UTF-8 存储 (含 UTF-16 转码副本) 保留完整正则语法 —— review 抓的回归：
    // 旧代码查 stats().encoding, 把 UTF-16 文件误踢进字面量分支，`ERROR|FATAL`
    // 变成逐字节字面匹配，命中恒空。
    // 两分支的 `(?i)` 前缀是 2026-09-15 的默认不敏感 (spec D2/D3)。
    assert_eq!(
        build_search_pattern(Encoding::Utf8, "ERROR|FATAL"),
        "(?i)ERROR|FATAL"
    );
    // GBK 存储：转字节 → \xNN 字面量 (退化为字面量语义), 外面套同一个 (?i)
    assert_eq!(
        build_search_pattern(Encoding::Gbk, "中文"),
        "(?i)(?-u)\\xD6\\xD0\\xCE\\xC4"
    );
}

#[test]
fn search_pattern_is_case_insensitive_by_default_with_opt_out() {
    // 默认不敏感：小写查询命中大写内容
    let re = regex::bytes::Regex::new(&build_search_pattern(Encoding::Utf8, "error")).unwrap();
    assert!(re.is_match(b"2026-09-15 ERROR boom"));
    // 逃逸舱：`(?-i)` 组内覆盖，用户想精确时零代码可用
    let re = regex::bytes::Regex::new(&build_search_pattern(Encoding::Utf8, "(?-i)error")).unwrap();
    assert!(!re.is_match(b"2026-09-15 ERROR boom"), "逃逸舱须恢复敏感");
    assert!(re.is_match(b"2026-09-15 error boom"));
}

#[test]
fn case_prefix_does_not_change_regex_class_semantics() {
    // `(?i)` 而非 `(?i-u)`: `\w` 仍是 Unicode 语义 (含汉字), `\d` 仍是 Unicode 数字。
    // 若误用 `(?-u)`, `\w` 会退化成 ASCII 类 —— 这两条断言就是那把锁。
    let re = regex::bytes::Regex::new(&build_search_pattern(Encoding::Utf8, r"\w+")).unwrap();
    assert!(re.is_match("汉字".as_bytes()), "\\w 必须仍是 Unicode 语义");
    let re = regex::bytes::Regex::new(&build_search_pattern(Encoding::Utf8, r"^\d+$")).unwrap();
    assert!(re.is_match("123".as_bytes()), "\\d 必须仍是 Unicode 数字类");
}

/// `parse_filter` 是过滤解析的**唯一入口**: 键名规范化只在有 schema 时发生，
/// 且用户输入的原文不改 (状态栏显示的是他敲的那串)。
#[test]
fn parse_filter_normalizes_keys_only_when_schema_present() {
    let mut app = LogApp::new_empty();
    let field = |p: &str| {
        vec![jsonl::Clause::Field {
            path: vec![p.to_string()],
            op: jsonl::Op::Eq,
            value: "ERROR".to_string(),
        }]
    };

    // .log (无 schema): 键名原样 —— 与改造前一字不差
    assert!(app.schema.is_none(), "new_empty 应无 schema");
    assert_eq!(app.parse_filter("LEVEL=ERROR"), field("LEVEL"));

    // JSONL (有 schema): 改写为列里的真实写法
    app.schema = Some(Arc::new(Schema {
        columns: vec![jsonl::Column {
            name: "level".into(),
            width_chars: 5,
        }],
    }));
    assert_eq!(app.parse_filter("LEVEL=ERROR"), field("level"));
    // 大小写已一致的照常通过
    assert_eq!(app.parse_filter("level=ERROR"), field("level"));
    // 裸词与多段路径不经规范化
    assert_eq!(
        app.parse_filter("boom"),
        vec![jsonl::Clause::Bare("boom".into())]
    );
    assert_eq!(
        app.parse_filter("a.b=1"),
        vec![jsonl::Clause::Field {
            path: vec!["a".into(), "b".into()],
            op: jsonl::Op::Eq,
            value: "1".into(),
        }]
    );
}

/// 过滤栏存的是**用户敲的原文**, 规范化只发生在解析层 —— 状态栏回显与
/// 清空重放 (`filter_applied`) 都拿它，改掉会让用户看到自己没敲过的字。
#[test]
fn apply_filter_keeps_raw_query_for_display() {
    let mut app = LogApp::new_empty();
    app.schema = Some(Arc::new(Schema {
        columns: vec![jsonl::Column {
            name: "level".into(),
            width_chars: 5,
        }],
    }));
    app.apply_filter("LEVEL=ERROR".to_string());
    assert_eq!(
        app.filter_applied, "LEVEL=ERROR",
        "存的是原文不是规范化结果"
    );
}

/// 模态键盘门禁 (2026-09-14 用户实机): 设置卡开着、卡内下拉未持焦时
/// 按 ↑↓, 卡底下的日志区滚动了。卡内控件经焦点路由消费、不经 `app.event`;
/// 能到这里的都是无人认领的键，除 Esc (关卡) 外一律吞掉。
#[test]
fn settings_modal_swallows_unhandled_keys() {
    let mut app = LogApp::new_empty();
    let lines: String = (0..100)
        .map(|i| format!("2026-09-14 12:00:{i:02} INFO line {i}\n"))
        .collect();
    let p = temp_log(lines.as_bytes());
    app.file = Arc::new(LogFile::open(&p).unwrap());
    app.has_file = true;
    app.settings_open = true;

    let key = |key: NamedKey| Event::Key {
        key: Key::Named(key),
        pressed: true,
        shift: false,
        ctrl: false,
        alt: false,
    };
    app.event(&key(NamedKey::ArrowDown));
    assert_eq!(app.top_row, 0.0, "设置卡开着：↓ 不得滚动底层日志");
    app.event(&key(NamedKey::PageDown));
    assert_eq!(app.top_row, 0.0, "设置卡开着：PageDown 不得滚动底层日志");

    // Esc 不在吞键范围：必须仍能关卡。
    app.event(&key(NamedKey::Escape));
    assert!(!app.settings_open, "Esc 必须仍能关闭设置卡");
    std::fs::remove_file(&p).ok();
}

/// 空态 LogApp 测试夹具 (与 run() 的空态骨架同构)。
fn temp_log(content: &[u8]) -> PathBuf {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "danqing-log-main-{}-{}.log",
        std::process::id(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    ));
    std::fs::write(&path, content).unwrap();
    path
}

/// 追加换入：计数由落点按「重叠一行」增量更新，终值必须等于对新文件的全量重算。
#[test]
fn apply_appended_updates_counts_incrementally() {
    let mut app = LogApp::new_empty();
    let p1 = temp_log("2026-09-05 12:00:01 ERROR one\n2026-09-05 12:00:02 INFO two\n".as_bytes());
    let f1 = LogFile::open(&p1).unwrap();
    app.level_counts = Arc::new(levels::count_levels(&f1));
    app.file = Arc::new(f1);
    app.levels_pending = false;
    assert_eq!(app.level_counts.get(Level::Error), 1, "起点 1 条 ERROR");

    let p2 = temp_log(
            "2026-09-05 12:00:01 ERROR one\n2026-09-05 12:00:02 INFO two\n2026-09-05 12:00:03 ERROR three\n"
                .as_bytes(),
        );
    let f2 = LogFile::open(&p2).unwrap();
    app.apply_appended(f2, None);

    assert_eq!(
        *app.level_counts.as_ref(),
        levels::count_levels(&app.file),
        "增量更新 == 对新文件全量重算"
    );
    assert_eq!(app.level_counts.get(Level::Error), 2, "旧 1 + 新 1");
    assert_eq!(app.level_counts.total(), 3);
    std::fs::remove_file(&p1).ok();
    std::fs::remove_file(&p2).ok();
}

/// **计数未就绪时追加**: 不得重起作业 —— 一个持续增长的 tail 会把计数
/// 一遍遍从头来过 (永远算不完，侧栏永远挂在「…」); 也不得把旧快照的增量
/// 贴到新文件上。正确做法是等作业交付时与快照对账 (`pickup_levels_job`)。
#[test]
fn apply_appended_while_pending_keeps_counts_pending() {
    let mut app = LogApp::new_empty();
    let p = temp_log(
        "2026-09-05 12:00:01 ERROR one
"
        .as_bytes(),
    );
    app.file = Arc::new(LogFile::open(&p).unwrap());
    app.levels_pending = true; // 模拟后台作业仍在算旧快照
    app.level_counts = Arc::new(LevelCounts::default());

    let f2 = LogFile::open(&p).unwrap();
    app.apply_appended(f2, None);
    assert!(app.levels_pending, "仍等原作业交付，不得重起");
    assert_eq!(
        app.level_counts.total(),
        0,
        "不得把旧快照的增量贴到新文件上"
    );
    std::fs::remove_file(&p).ok();
}

/// 作业快照之后文件又长过 → 交付时按「重叠一行」与快照对账，得到与**当前**
/// 文件一致的计数 (既不重起作业，也不交付一份过期的数)。
#[test]
fn pickup_levels_job_reconciles_lines_added_after_snapshot() {
    let mut app = LogApp::new_empty();
    let p = temp_log(
        "2026-09-05 12:00:01 ERROR one
"
        .as_bytes(),
    );
    app.file = Arc::new(LogFile::open(&p).unwrap());
    app.level_column = None;
    app.launch_levels_job(); // 快照 = 此刻的 1 行

    // 作业在算的同时文件又长了两行
    {
        let mut f = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
        f.write_all(
            b"2026-09-05 12:00:02 INFO two
2026-09-05 12:00:03 ERROR three
",
        )
        .unwrap();
    }
    app.file = Arc::new(LogFile::open(&p).unwrap());
    assert_eq!(app.file.line_count(), 3, "快照之后又长了两行");

    let deadline = Instant::now() + Duration::from_secs(5);
    while app.levels_pending {
        app.pickup_levels_job();
        assert!(Instant::now() < deadline, "计数作业 5s 未交卷");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        *app.level_counts.as_ref(),
        levels::counts_for(Arc::clone(&app.file), None).counts,
        "对账后 == 当前文件全量重算 (不是过期的那份)"
    );
    assert_eq!(app.level_counts.get(Level::Error), 2, "两条 ERROR 都在");
    assert_eq!(app.level_counts.total(), 3);
    std::fs::remove_file(&p).ok();
}

/// **R1 回归 (应用层)**: 旧快照末行无换行、被追加补全改判时，计数与过滤必须
/// **同时**覆盖那一行 —— 只改一侧就会让柱条数字与筛选结果分岔 (D2 红线)。
#[test]
fn apply_appended_covers_completed_half_line_on_both_sides() {
    let mut app = LogApp::new_empty();
    let p = temp_log("X\n2026-09-05 12:00:01 ".as_bytes());
    let f1 = LogFile::open(&p).unwrap();
    app.level_counts = Arc::new(levels::count_levels(&f1));
    app.file = Arc::new(f1);
    app.levels_pending = false;
    assert_eq!(app.level_counts.get(Level::Error), 0, "半行未成词");

    // 已应用的过滤 + 已建立 (空) 的过滤表，模拟 tail 中途
    app.filter_applied = "ERROR".into();
    app.filtered = Some(Arc::new(Vec::new()));

    {
        let mut f = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
        f.write_all(b"ERROR disk\n").unwrap();
    }
    let f2 = LogFile::open(&p).unwrap();
    assert_eq!(f2.line_count(), 2, "补全半行不增行数");
    app.apply_appended(f2, None);

    assert_eq!(
        app.level_counts.get(Level::Error),
        1,
        "被补全的半行必须改判"
    );
    assert_eq!(app.level_counts.get(Level::Other), 1, "只剩开头那行 X");
    let shown = app.filtered.as_ref().expect("过滤表仍在");
    assert_eq!(
        shown.len() as u64,
        app.level_counts.get(Level::Error),
        "D2 红线：筛出行数必须 == 柱条数字"
    );
    assert_eq!(
        shown.as_slice(),
        [1],
        "第 1 行 (0-based) 是那条补全的 ERROR"
    );
    std::fs::remove_file(&p).ok();
}
/// **T5 端到端一致性 (D2 红线的应用层落点)**: 点柱条 → 真实过滤管道 →
/// 筛出行数 == 柱条数字。
///
/// 引擎层的逐桶相等已由 `levels.rs` 的 `field_counts_equal_filter_hits_...`
/// 钉住; 这条补的是「应用层真的把对的那个子句发出去了」——
/// 子句生成、toggle 语义、AsyncJob 管道都不脱节。
#[test]
fn clicking_a_bar_filters_to_exactly_the_bar_count() {
    let mut app = LogApp::new_empty();
    // 300 行：ERROR / INFO / WARNING 各 100。WARNING 是关键样本 ——
    // 它验证别名靠前缀通配被吃到 (字节全等的 level=WARN 会筛出 0 行)。
    let mut content = Vec::new();
    for i in 0..300 {
        let lv = match i % 3 {
            0 => "ERROR",
            1 => "INFO",
            _ => "WARNING",
        };
        content.extend_from_slice(format!("{{\"level\":\"{lv}\",\"i\":{i}}}\n").as_bytes());
    }
    let p = temp_log(&content);
    let f = LogFile::open(&p).unwrap();
    app.level_counts = Arc::new(levels::count_levels_field(&f, "level"));
    app.level_queries = levels::level_queries_for("level");
    app.file = Arc::new(f);
    app.has_file = true;

    for level in [Level::Error, Level::Info, Level::Warn] {
        app.filter_applied.clear(); // 避开 toggle 分支，单纯验「套用后筛多少」
        app.apply_level_filter(level);
        let deadline = Instant::now() + Duration::from_secs(5);
        let lines = loop {
            if let Some(out) = app.filter_job.poll() {
                break out.lines;
            }
            assert!(Instant::now() < deadline, "过滤 job 5s 未交卷 (悬挂？)");
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(
            lines.len() as u64,
            app.level_counts.get(level),
            "{level:?}: 筛出行数 != 柱条数字 —— D2 红线在应用层破裂"
        );
        assert_eq!(lines.len(), 100, "{level:?}: 300 行三轮 → 各 100");
    }

    std::fs::remove_file(&p).ok();
}

/// **本次事故的回归 (2026-09-12)**: 打开**不得**等计数。
///
/// 事由：计数原先与文件同批交付，而字段口径的 `extract_field` 要在整行里找
/// `"level":`, 成本随行内容走 —— 用户实机打开 1GB JSONL 时，状态栏写
/// 「索引 92ms」却等了十几秒 (那十几秒全在 worker 里数级别)。
/// 修法：打开只交出口径列名，计数交独立后台作业，侧栏随后补入。
///
/// 这条钉住三件事：① 落地后侧栏处于「未就绪」而非拿 0 冒充; ② 此时只读
/// (不得拿空子句表去点); ③ 作业交付后计数等于全量重算。
#[test]
fn apply_fresh_does_not_block_on_level_counting() {
    // 注入配置路径：apply_fresh 现会读 state.json (T5), 无注入 panic 封死会拦
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("lvlcnt")));
    let p = temp_log(
        "{\"level\":\"ERROR\",\"m\":\"a\"}\n{\"level\":\"INFO\",\"m\":\"b\"}\n".as_bytes(),
    );
    let f = LogFile::open(&p).unwrap();
    let out = OpenOutcome {
        file: f,
        schema: jsonl::discover_schema(&LogFile::open(&p).unwrap()),
        incremental_hits: None,
        rebuilt: false,
        level_column: Some("level".into()),
    };
    app.apply_fresh(p.clone(), out);

    // ① 落地即返回：计数未就绪
    assert!(app.levels_pending, "打开不得等计数 —— 落地时计数必未就绪");
    assert_eq!(app.level_counts.total(), 0, "不得拿 0 冒充真实计数");
    // ② 未就绪期间侧栏只读 (空子句表), 点不到任何一行
    assert!(
        app.level_queries.iter().all(Option::is_none),
        "计数未就绪 → 侧栏只读"
    );

    // ③ 等后台作业交付
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.levels_pending {
        app.pickup_levels_job();
        assert!(Instant::now() < deadline, "计数作业 5s 未交卷 (悬挂？)");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        *app.level_counts.as_ref(),
        levels::counts_for(Arc::clone(&app.file), Some("level")).counts,
        "交付的计数 == 全量重算"
    );
    assert_eq!(app.level_counts.get(Level::Error), 1);
    assert_eq!(app.level_counts.get(Level::Info), 1);
    assert_eq!(
        app.level_queries[Level::Error as usize].as_deref(),
        Some("level=ERROR*"),
        "就绪后子句表随口径一起到位"
    );
    std::fs::remove_file(&p).ok();
}

/// **打开文件后必须有人持焦** (review 轮补的第四条): T14 把「选中行高亮」改成
/// 只在 `LogView` 持焦时才画 (`visible_selection` 的焦点门禁), 而 `Ctrl+O`
/// 打开文件后**没有任何控件**持焦 —— 于是打开 1GB 日志看到的是「一行都没选中」,
/// 按 ↑↓ 只动底栏行号、屏上什么都不动：正是本模块判据① (按了有没有立刻的
/// 变化) 要消灭的那一类。修法是 `apply_fresh` 尾部把焦点送进列表。
///
/// **两半都要钉**: Fresh 送、append 不送。后者若也送，后台追长 / 轮转会把正在
/// 过滤栏里打字的用户当场拽走 (焦点一挪，接下来敲的字就不进栏了)。
#[test]
fn fresh_open_hands_focus_to_the_list_but_append_does_not() {
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("freshfocus")));
    let p = temp_log("2026-09-05 12:00:01 ERROR one\n".as_bytes());
    let out = OpenOutcome {
        file: LogFile::open(&p).unwrap(),
        schema: None,
        incremental_hits: None,
        rebuilt: false,
        level_column: None,
    };

    // 打开文件 (Fresh): 焦点必须送进列表
    app.focus_target = Some("log-bar"); // 假装用户正停在过滤栏
    app.apply_fresh(p.clone(), out);
    assert_eq!(
        app.focus_target,
        Some("log-view"),
        "打开文件后须把焦点送进列表 —— T14 起高亮只在持焦时画，不送就是「一行没选中」"
    );

    // 对照：**追加**不得动焦点 (用户可能正在栏里打字)
    app.focus_target = Some("log-bar");
    app.level_counts = Arc::new(levels::count_levels(&app.file));
    app.levels_pending = false;
    let p2 = temp_log("2026-09-05 12:00:01 ERROR one\n2026-09-05 12:00:02 INFO two\n".as_bytes());
    app.apply_appended(LogFile::open(&p2).unwrap(), None);
    assert_eq!(
        app.focus_target,
        Some("log-bar"),
        "追长/轮转不得抢焦点 —— 否则正在过滤栏里打的字当场丢失"
    );

    std::fs::remove_file(&p).ok();
    std::fs::remove_file(&p2).ok();
}

/// 轮转/重建: 计数重新后台算，且列名与子句表跟着换 —— JSONL 变明文后必须
/// 降级只读，不能留着旧列名的子句去点 (会筛出 0 行)。
#[test]
fn apply_rebuild_switches_column_and_recounts() {
    let mut app = LogApp::new_empty();
    let p1 = temp_log(
        "{\"level\":\"ERROR\",\"m\":\"a\"}\n{\"level\":\"INFO\",\"m\":\"b\"}\n".as_bytes(),
    );
    let f1 = LogFile::open(&p1).unwrap();
    app.level_counts = Arc::new(levels::count_levels_field(&f1, "level"));
    app.level_queries = levels::level_queries_for("level");
    app.level_column = Some("level".into());
    app.file = Arc::new(f1);
    app.has_file = true;
    assert!(
        app.level_queries[Level::Error as usize].is_some(),
        "起点：JSONL 可点"
    );

    // 轮转后内容变明文 → 口径列随之作废
    let p2 = temp_log("2026-09-05 ERROR plain one\n2026-09-05 WARN plain two\n".as_bytes());
    let out = OpenOutcome {
        file: LogFile::open(&p2).unwrap(),
        schema: None,
        incremental_hits: None,
        rebuilt: true,
        level_column: None,
    };
    app.apply_rebuild(&p2, out);

    assert!(app.levels_pending, "重建后计数重新后台算");
    assert!(app.level_column.is_none(), "列名换掉，不沿用旧的");
    assert!(
        app.level_queries.iter().all(Option::is_none),
        "明文 → 子句表清空 (降级只读)"
    );

    let deadline = Instant::now() + Duration::from_secs(5);
    while app.levels_pending {
        app.pickup_levels_job();
        assert!(Instant::now() < deadline, "计数作业 5s 未交卷");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        *app.level_counts.as_ref(),
        levels::counts_for(Arc::clone(&app.file), None).counts,
        "重建后计数 == 新文件全量重算"
    );
    assert_eq!(app.level_counts.get(Level::Error), 1);
    assert_eq!(app.level_counts.get(Level::Warn), 1);
    assert_eq!(app.level_counts.total(), 2);
    std::fs::remove_file(&p1).ok();
    std::fs::remove_file(&p2).ok();
}
#[test]
fn apply_fresh_invalidates_inflight_filter_and_search_jobs() {
    // review C1 回归：旧文件上的在途 filter/search 结果，换入新文件后
    // 不得贴上 (worker 用通道闸门控制交付时序，无运气成分)
    let mut app = LogApp::new_empty_at(Some(temp_cfg_path("freshinv")));
    let (f_tx, f_rx) = std::sync::mpsc::channel::<()>();
    app.filter_job.launch(move || {
        f_rx.recv().ok();
        FilterOutcome {
            lines: vec![1, 2],
            elapsed: Duration::ZERO,
        }
    });
    let (s_tx, s_rx) = std::sync::mpsc::channel::<()>();
    app.search_job.launch(move || {
        s_rx.recv().ok();
        SearchOutcome {
            hits: vec![5],
            total: 1,
            elapsed: Duration::ZERO,
            pattern: "x".into(),
            query: "x".into(),
        }
    });
    // 两个 worker 阻塞中 (结果必未到达) → 换入新文件 (invalidate 发生)
    let p = temp_log(b"new\nfile\n");
    let f = LogFile::open(&p).unwrap();
    let out = OpenOutcome {
        file: f,
        schema: None,
        incremental_hits: None,
        rebuilt: false,
        level_column: None,
    };
    app.apply_fresh(p.clone(), out);
    // 放行 worker 交付，长窗轮询：结果必须永不到达
    f_tx.send(()).unwrap();
    s_tx.send(()).unwrap();
    let mut filter_got = false;
    let mut search_got = false;
    for _ in 0..100 {
        filter_got |= app.filter_job.poll().is_some();
        search_got |= app.search_job.poll().is_some();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!filter_got, "在途过滤结果必须被 invalidate 丢弃");
    assert!(!search_got, "在途搜索结果必须被 invalidate 丢弃");
    assert!(app.filtered.is_none());
    assert!(app.search.is_none());
    std::fs::remove_file(&p).ok();
}

// ---- M3 (T12): 九条沉默接入的回归锁 ----
//
// 公共判据是「触发后**出声**且**说得对**」: 只测「有提示」会放过提示写错原因
// 的形态，只测「返回 Consumed/Ignored」则完全测不出这一批改动 (M3 一律不动
// 归因，只加原因)。

/// 敲一个无修饰键。
fn press(app: &mut LogApp, key: Key) {
    app.event(&Event::Key {
        key,
        pressed: true,
        shift: false,
        ctrl: false,
        alt: false,
    });
}

/// 敲一个 Ctrl+字母。
fn press_ctrl(app: &mut LogApp, ch: &str) {
    app.event(&Event::Key {
        key: Key::Character(ch.to_string()),
        pressed: true,
        shift: false,
        ctrl: true,
        alt: false,
    });
}

/// 造一个开了真文件 (内容自定) 的 app, 交给 `f` 跑，收尾删文件。
/// 走 `has_file` 门禁**之后**的键处理路径 —— 空态会先被 P26 那条拦下。
fn with_file<T>(content: &[u8], f: impl FnOnce(&mut LogApp) -> T) -> T {
    let p = temp_log(content);
    let mut app = LogApp::new_empty();
    app.file = Arc::new(LogFile::open(&p).unwrap());
    app.has_file = true;
    let out = f(&mut app);
    std::fs::remove_file(&p).ok();
    out
}

/// 取当前 notice 文本; 没有则连底栏一起报出来 (方便定位是「没出声」还是
/// 「声出到了别处」)。
fn notice_of(app: &LogApp) -> String {
    app.notice
        .as_ref()
        .map(|(t, _)| t.clone())
        .unwrap_or_else(|| panic!("须有一条 notice; 当前底栏：{}", app.status))
}

/// 敲一个 Ctrl+字符键事件 (不经过 app, 供 `app_key_filter` 直调)。
fn ctrl_key(ch: &str) -> Event {
    Event::Key {
        key: Key::Character(ch.to_string()),
        pressed: true,
        shift: false,
        ctrl: true,
        alt: false,
    }
}

/// T17 验收 ③: 拖滚动条**不改选中行** —— 抓条是「看」不是「选」。
///
/// 对照在同一支测试里：方向键那条路**会**动选中 (既有行为，本项不动它)。
/// 有对照才说明这条不变量是**有意**分开的，不是碰巧没动。
#[test]
fn scroll_to_does_not_move_the_selection() {
    let body: String = (0..300).map(|i| format!("line {i}\n")).collect();
    let (mut app, p) = {
        let p = temp_log(body.as_bytes());
        let mut app = LogApp::new_empty();
        app.file = Arc::new(LogFile::open(&p).unwrap());
        app.has_file = true;
        (app, p)
    };
    app.selected = 7;

    app.update(Msg::ScrollTo {
        top: 120.0,
        at_bottom: false,
    });
    assert_eq!(app.top_row, 120.0, "绝对定位须原样落到 top_row");
    assert_eq!(app.selected, 7, "抓滚动条不得动选中行");

    app.update(Msg::ScrollRows(1.0));
    assert_eq!(app.selected, 121, "对照：滚轮/方向键那条路仍让选中跟随首行");
    std::fs::remove_file(&p).ok();
}

/// T17 回归锁：在底部**碰一下滚动条**不得静默脱掉 FOLLOW。
///
/// 跟随态下 app 的 `top_row` 是 `count-1`, 而滚动条能表达的最大值是
/// `count-可见行数` —— 两个口径差着 `可见行数 -1` 行。原先直接比
/// `top < top_row`, 于是「在底部往下拖」也被判成「向上看」, ` · FOLLOW`
/// 悄悄从底栏消失。修法：落到条底 (`at_bottom`) 时不参与这个判断、
/// 往回拖仍照旧。**A/B 实证**: 去掉 `!at_bottom` 那一项，本测试第一段必红。
#[test]
fn touching_the_scroll_bar_at_the_bottom_keeps_follow() {
    let body: String = (0..300).map(|i| format!("line {i}\n")).collect();
    let p = temp_log(body.as_bytes());
    let mut app = LogApp::new_empty();
    app.file = Arc::new(LogFile::open(&p).unwrap());
    app.has_file = true;

    // 条能表达的最底位比 app 的 `max_top()` 小 —— 正是当年误判的那一格
    let bar_bottom = app.max_top() - 10.0;
    app.follow = true;
    app.top_row = app.max_top();
    app.update(Msg::ScrollTo {
        top: bar_bottom,
        at_bottom: true,
    });
    assert!(app.follow, "拖到条底不该脱跟随 (用户没在往回看)");

    // 对照：真往回拖 (没到条底) 仍然脱跟随。**必须把 top_row 放回跟随位**
    // —— 上一条已经把 top_row 拉到了 bar_bottom, 不放回去这条就不是「往回拖」。
    app.follow = true;
    app.top_row = app.max_top();
    app.update(Msg::ScrollTo {
        top: bar_bottom,
        at_bottom: false,
    });
    assert!(!app.follow, "往回拖须停止跟随");
    std::fs::remove_file(&p).ok();
}

/// P30 (T17): **未认领的滚轮**转给列表滚动 —— 指针停在侧栏/过滤栏上时,
/// 日志区收不到滚轮 (框架按点子命中分发、不向父级回落), 原先必须把指针挪回
/// 内容区才滚得动。
///
/// 判据取**效果** (top_row 动了) 而不是「有没有走那个分支」。
#[test]
fn unclaimed_wheel_scrolls_the_list() {
    let body: String = (0..300).map(|i| format!("line {i}\n")).collect();
    let p = temp_log(body.as_bytes());
    let mut app = LogApp::new_empty();
    app.file = Arc::new(LogFile::open(&p).unwrap());
    app.has_file = true;

    let wheel = |dy: f32| Event::MouseWheel {
        delta: (0.0, dy),
        position: danqing::Point::new(10.0, 400.0), // 侧栏上
        shift: false,
        ctrl: false,
        alt: false,
    };
    app.event(&wheel(-1.0)); // 向下滚 (与 view 同向：delta.1 < 0 = 向下)
    assert!(app.top_row > 0.0, "侧栏上的滚轮须滚得动列表");

    // 模态不穿透：卡开着时滚轮只属于卡
    let before = app.top_row;
    app.settings_open = true;
    app.event(&wheel(-1.0));
    assert_eq!(app.top_row, before, "设置卡开着时滚轮不得滚卡后的日志");
    std::fs::remove_file(&p).ok();
}

/// T17: 滚轮换算**单点** —— 内容区与「未认领」那一路必须是同一个手感。
///
/// 顺带钉住上界：框架把 `LineDelta`(行) 与 `PixelDelta`(像素) 抹平成同一个
/// `f32` (G10), 触控板一次给 ±100 时若不夹，一滚就跳几百行。
#[test]
fn wheel_rows_is_clamped_and_sign_flipped() {
    assert!(wheel_rows(-1.0) > 0.0, "与 danqing Scrollable 同向");
    assert_eq!(wheel_rows(0.0), 0.0);
    // 钉**值**而不是钉「有夹子」: 写 `<= WHEEL_MAX_ROWS` 的话，把常量从 12
    // 改成 50 它照样绿 —— 那就不叫守卫了。
    assert_eq!(wheel_rows(-100.0), WHEEL_MAX_ROWS, "像素档夹到上界");
    assert_eq!(wheel_rows(100.0), -WHEEL_MAX_ROWS);
}

/// T18 (P17): notice 自带消退期限 —— 到点清掉，不留常驻噪声。
///
/// 不真等 4 秒：把期限拨到过去，语义等价。
#[test]
fn notice_expires_when_its_deadline_passes() {
    let mut app = LogApp::new_empty();
    app.set_notice("已复制该行".into(), NoticeKind::Info);
    assert!(app.notice.is_some());
    app.expire_notice();
    assert!(app.notice.is_some(), "未到点不得消退");

    app.notice_until = Some(Instant::now() - Duration::from_millis(1));
    app.expire_notice();
    assert!(app.notice.is_none(), "到点须消退");
    assert!(app.notice_until.is_none(), "期限也要一并清掉");
}

/// SPEC-notice-visibility T2: 点掉 toast = 与到点消退同途 (dismiss_notice 收口),
/// notice 双清。
#[test]
fn dismiss_notice_clears_notice_like_expiry() {
    let mut app = LogApp::new_empty();
    app.set_notice("偏移须是毫秒整数".into(), NoticeKind::Warn);
    assert!(app.notice.is_some());
    app.update(Msg::DismissNotice);
    assert!(app.notice.is_none(), "点掉须清 notice");
    assert!(app.notice_until.is_none(), "期限一并清");
}

/// SPEC-notice-visibility T2 (非模态铁律): Warn 浮层在屏时 `popover_open()` 仍 false
/// —— toast 不进模态门控清单 ( Esc 次序/滚轮门禁/Ctrl 守卫的共同判据),
/// 否则 toast 一弹全窗口的键盘滚轮都被模态语义拦掉。
#[test]
fn warn_toast_does_not_count_as_popover() {
    let mut app = LogApp::new_empty();
    app.set_notice("x".into(), NoticeKind::Warn);
    assert!(
        !app.popover_open(),
        "toast 是浮层不是弹层：不得污染模态门控"
    );
}

/// 分档口径锁 (2026-09-29 全仓大盘点，SPEC-notice-visibility §11):
/// 「你按的那下没生效」类提示必须是 **Warn** (上 toast 浮层), 错给 Info =
/// 沉回底栏 —— 用户实机 H-a 验收当场撞出第一条 (无选中源点步进是 Info)。
/// 锁代表样本三族; 全量 23 处口径表在 spec §11, 新加提示先过表。
#[test]
fn ineffective_action_notices_are_warn_not_info() {
    // 指针语义族：无选中源改时间参数 (H-a 原始场景)
    let mut app = LogApp::new_empty();
    app.edit_source_time(|o, t| (Some(o + 1), Some(t)));
    assert!(
        matches!(app.notice, Some((_, NoticeKind::Warn))),
        "无选中源改时间参数 = 没生效，必须 Warn: {:?}",
        app.notice
    );
    // 指针语义族：无选中源点移除
    let mut app = LogApp::new_empty();
    app.remove_selected_merge_source();
    assert!(
        matches!(app.notice, Some((_, NoticeKind::Warn))),
        "无选中源点移除 = 没生效，必须 Warn: {:?}",
        app.notice
    );
    // 跳转无对象族：无书签按 ' 跳下一书签
    let mut app = LogApp::new_empty();
    app.goto_next_bookmark();
    assert!(
        matches!(app.notice, Some((_, NoticeKind::Warn))),
        "无书签跳转 = 没生效，必须 Warn: {:?}",
        app.notice
    );
    // 前置不满足族：无文件按 b 夹书签
    let mut app = LogApp::new_empty();
    app.toggle_bookmark();
    assert!(
        matches!(app.notice, Some((_, NoticeKind::Warn))),
        "无文件夹书签 = 没生效，必须 Warn: {:?}",
        app.notice
    );
    // 前置不满足族：打开 A 追加源还选 A (去重后只剩主源, 起并不成) ——
    // 2026-09-29 用户实机报的漏网条 (首盘多行调用跳读漏列, 复查抓回)
    let (mut app, cur, _a, _b) = merge_fixture("kind-start");
    open_single(&mut app, &cur);
    app.add_merge_source(cur.clone());
    assert!(
        matches!(&app.notice, Some((t, NoticeKind::Warn)) if t.contains("至少需要两个源")),
        "起并选重复源 = 没生效，必须 Warn: {:?}",
        app.notice
    );
    std::fs::remove_file(&cur).ok();
    // 功能不可用族：非合并态发起追踪
    let mut app = LogApp::new_empty();
    app.start_trace(0, 0, 0, 1);
    assert!(
        matches!(&app.notice, Some((t, NoticeKind::Warn)) if t.contains("合并视图")),
        "非合并态追踪 = 没生效，必须 Warn: {:?}",
        app.notice
    );
}

/// D 闸 (2026-09-29 用户裁定, G 组验收缺陷②): 只剩两源点「移除」**不出手**
/// —— 源一个不动、合并不退出、提示指路「退出合并」。
/// 修复前: 2 源移除走「不足两源退出合并」分支 —— 移除其实生效了, 但弹层
/// 行列表零可见变化 (bundle 保留), 用户读作「移除不了」(实机截图: 行原样,
/// 只有按钮悄悄变「返回合并」)。「移除」不再承诺它做不到的事。
#[test]
fn remove_with_only_two_sources_is_refused_and_points_to_exit() {
    let (mut app, cur, a, _b) = merge_fixture("d-gate");
    open_single(&mut app, &cur);
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone()]);
    assert_eq!(app.merge.as_ref().unwrap().sources.len(), 2);
    app.merge_source_selected = Some(a.to_string_lossy().into_owned());

    app.remove_selected_merge_source();

    let m = app.merge.as_ref().expect("D 闸: 合并不退出, bundle 不动");
    assert_eq!(m.sources.len(), 2, "D 闸: 源一个不动");
    assert_eq!(app.workspace, Workspace::Merge, "D 闸: 不触发退出合并");
    let Some((text, kind)) = &app.notice else {
        panic!("D 闸拦下须出声指路")
    };
    assert!(
        matches!(kind, NoticeKind::Warn) && text.contains("退出合并"),
        "提示须指路「退出合并」: {text}"
    );
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
}

/// D 闸不挡正路: ≥3 源移除正常删 (重归并交付后源数减一)。
#[test]
fn remove_with_three_sources_still_works() {
    let (mut app, cur, a, b) = merge_fixture("d-pass");
    open_single(&mut app, &cur);
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone(), b.clone()]);
    assert_eq!(app.merge.as_ref().unwrap().sources.len(), 3);
    app.merge_source_selected = Some(a.to_string_lossy().into_owned());

    app.remove_selected_merge_source();
    pump_merge(&mut app);

    let m = app.merge.as_ref().unwrap();
    assert_eq!(m.sources.len(), 2, "≥3 源正常删: 源数减一");
    assert!(
        !m.sources
            .iter()
            .any(|s| s.path.to_string_lossy().contains("d-pass-a")),
        "被选中那条真被删掉"
    );
    assert!(app.merge_source_selected.is_none(), "移除后清选中指针");
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

/// SPEC-notice-visibility T4 (弹层之上，端到端行为锁): 模态弹层开着时，
/// 点击 toast 位置**先到 toast** (DismissNotice 出队), 弹层收不到这次点击
/// (不出 CloseMergeMenu —— scrim 点击本会关弹层)。
///
/// 构造保证：toast 挂 Stack 末位 + 框架事件反序分发 (`stack.rs` rev) +
/// 真实鼠标分发主链 `handler.rs:993` 就是 `tree.event` —— 本测试复刻真实路径。
/// ⚠ 点位常量与 `toast.rs` 同源声明：y = 窗底 − STATUS_HEIGHT − BOTTOM_GAP(8)
/// − TOAST_H/2(18); 改 toast 几何常量会红这里 (红是安全方向，来同步即可)。
#[test]
fn toast_gets_the_click_before_modal_popover() {
    use danqing::event::MouseButton;
    use danqing::widget::{EventResult, MsgQueue};
    use danqing::{Constraints, Point, Rect, RectBatch, TextBatch};

    let mut app = LogApp::new_empty();
    app.merge_menu_open = true; // 模态弹层开态 (直写字段构态，不触门控)
    app.set_notice("偏移须是毫秒整数 (如 -3000)".into(), NoticeKind::Warn);
    let mut tree = app.view();
    tree.sync(&app);
    let mut texts = TextBatch::default();
    let mut rects = RectBatch::new();
    let area = Rect::from_xywh(0.0, 0.0, 1280.0, 800.0);
    let size = tree.layout(Constraints::tight(area.size), &mut texts);
    tree.paint(Rect::new(Point::ZERO, size), &mut rects, &mut texts);

    let click = Event::MouseInput {
        button: MouseButton::Left,
        pressed: true,
        // 水平窗中 (toast 水平居中，中点必落在其上); y 见函数头同源声明。
        position: Point::new(640.0, 800.0 - crate::view::STATUS_HEIGHT - 8.0 - 18.0),
    };
    let mut msgs = MsgQueue::new();
    let result = tree.event(&click, area, &mut msgs);
    assert_eq!(
        result,
        EventResult::Consumed,
        "toast 命中须消费 (弹层收不到)"
    );
    assert!(
        msgs.iter()
            .any(|m| matches!(m.downcast_ref::<Msg>(), Some(Msg::DismissNotice))),
        "点击 toast 须出 DismissNotice"
    );
    assert!(
        !msgs
            .iter()
            .any(|m| matches!(m.downcast_ref::<Msg>(), Some(Msg::CloseMergeMenu))),
        "弹层不得吃到这次点击 (scrim 点击本会关弹层)"
    );
}

/// T20 (P37): 侧栏开关与 Ctrl+L 是**同一份状态** —— 两条入口同发一条消息。
///
/// 「双向同步」不是两处赋值互相对，而是只有一份真相：开关读
/// `app.histogram_visible`、Ctrl+L 与开关都发 `Msg::ToggleHistogram`。
///
/// **配置路径必须显式给临时文件**: 这条消息会 `save_config()`, 而默认路径是
/// 用户真实的 `config.toml` (整文件覆盖写)。本测试第一版就是这么写的，被
/// review 抓出来 —— 现在 `save_config` 在测试里拿不到路径会直接 panic。
#[test]
fn histogram_toggle_is_one_state_for_both_entries() {
    let p = temp_log(b"");
    let mut app = LogApp::new_empty_at(Some(p.clone()));
    let before = app.histogram_visible;
    app.update(Msg::ToggleHistogram);
    assert_eq!(app.histogram_visible, !before, "两条入口共用的那一支须翻转");
    app.update(Msg::ToggleHistogram);
    assert_eq!(app.histogram_visible, before, "再切一次回到原状");
    // 落盘也走的是**同一个**路径 (整文件同源，不碰用户真配置)
    assert_eq!(
        config::Config::load_from(&p).histogram,
        before,
        "切换须落盘到注入的路径"
    );
    std::fs::remove_file(&p).ok();
}

/// T21 (P39): Ctrl+F **不清草稿** —— 它是「回到搜索框」的反射键，
/// 而原实现每次都销毁用户已输入未应用的内容。
///
/// 这里锁的是**应用侧那一半** (不再发清空信号、改发重聚焦信号);
/// 「全选」那一半在 `view.rs::refocus_selects_the_existing_draft`。
#[test]
fn reopen_search_keeps_the_draft() {
    let mut app = LogApp::new_empty();
    let clear0 = app.search_clear_rev;
    let refocus0 = app.search_refocus_rev;

    app.open_search();
    assert_eq!(app.search_clear_rev, clear0, "Ctrl+F 不得再触发清空");
    assert_eq!(app.search_refocus_rev, refocus0 + 1, "改为请栏处理重聚焦");
    assert_eq!(app.focus_target, Some("log-bar"), "仍要把焦点送进栏");

    // 对照：Esc 那条路**仍然**清空 (本项只动「回到搜索框」这一条)
    app.clear_search();
    assert_eq!(app.search_clear_rev, clear0 + 1, "Esc 仍须清空");
}

/// T16 (P32) 回归锁：设置卡开着时，全局键**不得穿透到卡后**。
///
/// 逐条断言**卡后副作用没发生**, 而不是断言返回值是不是某个 `Msg` ——
/// 「不穿透」的实现可以是吞掉、也可以是别的，判据应该绑在**后果**上。
///
/// **Ctrl+O 不在表内 (有意)**: 它的穿透后果是弹一个**阻塞的原生文件对话框**,
/// 一旦回归，这条测试不是红而是**挂住** —— 一个会挂死的守卫比没有守卫更坏。
/// 它的门禁与表内三条是同一个 `if`, 位置对了三条就都对了; 实机那一半由
/// 矩阵 §6 的「卡开着按 Ctrl+O」(P32) 覆盖。
#[test]
fn settings_modal_does_not_leak_global_keys_behind_the_card() {
    let mut app = LogApp::new_empty();
    app.settings_open = true;
    for (name, ch) in [("Ctrl+F", "f"), ("Ctrl+T", "t"), ("Ctrl+L", "l")] {
        let got = app.app_key_filter(&ctrl_key(ch));
        let leaked = match got {
            Some(Msg::FocusSearch) => "把焦点送到了卡后",
            Some(Msg::ToggleMode) => "切了卡后的模式",
            Some(Msg::ToggleHistogram) => "动了卡后的侧栏",
            Some(Msg::OpenFile(_)) => "弹了文件对话框",
            _ => "",
        };
        assert!(leaked.is_empty(), "{name} 穿透了设置卡：{leaked}");
    }
    // **反向对照 —— 本测试第一版缺的就是这半边，于是漏掉了一个 Critical**:
    // 卡内控件 (主题下拉 / 侧栏开关 / 关闭钮) 全靠**焦点分发**收键，而框架在
    // `app_key_filter` 返回 `Some` 时直接 return、不再分发 (`handler.rs:436-441`)。
    // 所以非全局键**必须**放行 (`None`), 否则卡内键盘全死 —— 而上一段
    // 「不该发生的副作用没发生」是**看不出**这一点的 (吞得越多它越绿)。
    let plain = |k: Key| Event::Key {
        key: k,
        pressed: true,
        shift: false,
        ctrl: false,
        alt: false,
    };
    for (name, ev) in [
        ("↓ (下拉导航)", plain(Key::Named(NamedKey::ArrowDown))),
        (
            "Enter (下拉选中 / 关闭钮)",
            plain(Key::Named(NamedKey::Enter)),
        ),
        ("Space (侧栏开关)", plain(Key::Named(NamedKey::Space))),
        ("普通字符 (打字)", plain(Key::Character("x".into()))),
    ] {
        assert!(
            app.app_key_filter(&ev).is_none(),
            "{name} 必须放行给卡内控件 —— 守卫吞了它，卡内键盘就死了"
        );
    }
    // 这三条由 `LogApp::event` 那条路认领，那里有同款门禁 —— 一并锁住
    app.event(&ctrl_key("b"));
    app.event(&ctrl_key("g"));
    assert!(app.bookmarks.is_empty(), "Ctrl+B 不得在卡后加书签");
    // Esc 仍须能关卡 (既有行为保持，不被守卫吃掉)
    assert!(
        matches!(
            app.app_key_filter(&Event::Key {
                key: Key::Named(NamedKey::Escape),
                pressed: true,
                shift: false,
                ctrl: false,
                alt: false,
            }),
            Some(Msg::CloseSettings)
        ),
        "Esc 必须仍能关闭设置卡"
    );
}

/// T11 回归锁：notice 是**第二条通道**, 不得拼进 `status` 串。
///
/// 曾把它拼进去，而 `LogView::paint` 又单独画一遍 `notice` —— 同一句话在底栏
/// 出现**两次**。而两遍还是同色的，第一眼只像「重复」不像「出错」, 所以这条
/// 必须由守卫而不是靠眼睛。
#[test]
fn notice_is_not_folded_into_the_status_line() {
    let mut app = LogApp::new_empty();
    app.refresh_status();
    app.notice = Some(("此处无行".into(), NoticeKind::Info));
    app.refresh_status();
    assert!(app.notice.is_some(), "前提：notice 还在");
    assert!(
        !app.status.contains("此处无行"),
        "notice 不得拼进 status (会被画两遍): {}",
        app.status
    );
    // 反向对照：常态信息该在的仍在 (别把整条底栏一起删了)
    assert!(!app.status.is_empty(), "常态信息不得一起被删掉");
}

/// P27 收口 (2026-09-15 用户裁定「**后者**」): 无效正则这类错误**走 status 的
/// 错误态 (常驻红)**, **不进 notice 通道** —— notice 有 4 秒消退期，而它是
/// 「你刚按的那下没生效」, 不该自己消失。
///
/// 这条钉的是**标志与内容同真同假**: 置了错误态就得真有错误，换了常态就得清掉
/// —— 脱钩了就是「绿水配红字」那类假信息。
#[test]
fn invalid_regex_marks_the_status_as_an_error_and_normal_status_clears_it() {
    let mut app = LogApp::new_empty();
    app.refresh_status();
    assert!(!app.status_error, "起点不该是错误态");

    // 未闭合分组 → 正则必然编译失败
    app.apply_search("(".to_string());
    assert!(app.status.contains("正则无效"), "须说清为什么搜不了");
    assert!(
        app.status_error,
        "无效正则须把底栏标成错误 (paint 才会用 danger)"
    );
    assert!(
        app.notice.is_none(),
        "错误**不得**走 notice 通道 —— 那条 4 秒就消退，用户裁定要常驻"
    );
    // **反向对照**: 常态刷新必须清掉标志 —— 否则红字会跟着后续所有信息一起红
    app.refresh_status();
    assert!(!app.status_error, "常态刷新须清掉错误态");
    assert!(!app.status.contains("正则无效"), "常态刷新也该换掉那句话");
}

/// T13 —— 「被吞掉的输入必有原因」: **本表就是规则的挂载点**。
///
/// M3 之前本仓只有一处出声 (正则无效进底栏), 其余静默。这条把那个孤例**升格
/// 为规则**: 再遇到「按了没反应」, 要做的不是「记得去加提示」, 而是**往这张表
/// 加一行** —— 行在，判据在，原因就漏不掉。
///
/// 每行给的是**期望的原因片段**, 不是「有提示就算」: 后者会放过原因写错的形态。
/// 本表只收**键盘路径**上的吞键; 鼠标路径各有同构的守卫
/// (`view.rs::click_below_the_last_row_says_there_is_no_row` /
/// `expand_glyph_on_a_leaf_row_says_why_instead_of_toggling` /
/// `histogram.rs::readonly_sidebar_swallows_clicks_but_says_why`)。
#[test]
fn every_swallowed_key_says_why() {
    type Run = Box<dyn Fn() -> String>;
    let rows: Vec<(&str, &str, Run)> = vec![
        (
            "P26 空态按键",
            "尚未打开文件",
            Box::new(|| {
                let mut app = LogApp::new_empty();
                assert!(!app.has_file);
                press(&mut app, Key::Named(NamedKey::ArrowDown));
                notice_of(&app)
            }),
        ),
        (
            "P23 Ctrl+T 无 JSONL",
            "非 JSONL",
            Box::new(|| {
                with_file(b"2026-09-14 12:00:00 INFO ready\n", |app| {
                    assert!(app.schema.is_none(), "前提：无 JSONL schema");
                    press_ctrl(app, "t");
                    notice_of(app)
                })
            }),
        ),
        (
            "P24 ← 落在未展开行",
            "未展开",
            Box::new(|| {
                with_file(b"{\"a\":{\"b\":1}}\nplain\n", |app| {
                    app.mode = ViewMode::Table;
                    press(app, Key::Named(NamedKey::ArrowLeft));
                    notice_of(app)
                })
            }),
        ),
        (
            "P24 → 落在已展开行",
            "已展开",
            Box::new(|| {
                with_file(b"{\"a\":{\"b\":1}}\nplain\n", |app| {
                    app.mode = ViewMode::Table;
                    app.toggle_expand(0); // 真展开 (行 0 含嵌套)
                    assert!(app.expanded.is_expanded(0), "前提：行 0 已展开");
                    press(app, Key::Named(NamedKey::ArrowRight));
                    notice_of(app)
                })
            }),
        ),
        (
            "P25 Ctrl+G 无书签",
            "尚无书签",
            Box::new(|| {
                with_file(b"2026-09-14 12:00:00 INFO ready\n", |app| {
                    assert!(app.bookmarks.is_empty(), "前提：无书签");
                    press_ctrl(app, "g");
                    notice_of(app)
                })
            }),
        ),
        (
            // 口径须与 `ROADMAP-v1x.md` §四 同源 —— 写成「暂不支持」之类
            // 就又是一处各说各话，故断言片段锁在 "v1.x" 上。
            "P34 Ctrl+A 被吞",
            "v1.x",
            Box::new(|| {
                with_file(b"2026-09-14 12:00:00 INFO ready\n", |app| {
                    press_ctrl(app, "a");
                    notice_of(app)
                })
            }),
        ),
    ];
    for (name, expect, run) in &rows {
        let text = run();
        assert!(
            text.contains(expect),
            "{name}: 被吞掉的输入须说出「{expect}」, 实得「{text}」"
        );
    }
}

// ---- merge-timeline 腿一 T3: 合并工作区行为锁 ----

/// 合并 fixture: cur (单文件现场，3 行 ISO) + 源 a (.log ISO) + 源 b (.jsonl)。
fn merge_fixture(tag: &str) -> (LogApp, PathBuf, PathBuf, PathBuf) {
    let app = LogApp::new_empty_at(Some(temp_cfg_path(tag)));
    let dir = std::env::temp_dir();
    let cur = dir.join(format!("danqing-mt-{tag}-cur.log"));
    let a = dir.join(format!("danqing-mt-{tag}-a.log"));
    let b = dir.join(format!("danqing-mt-{tag}-b.jsonl"));
    std::fs::write(
            &cur,
            b"2026-09-27T00:00:00Z INFO c0\n2026-09-27T00:00:06Z INFO c1\n2026-09-27T00:00:07Z INFO c2\n",
        )
        .unwrap();
    std::fs::write(
            &a,
            b"2026-09-27T00:00:01Z INFO a0\n2026-09-27T00:00:03Z INFO a1\n2026-09-27T00:00:08Z INFO a2\n",
        )
        .unwrap();
    std::fs::write(
        &b,
        br#"{"ts":"2026-09-27T00:00:02Z","msg":"b0"}
{"ts":"2026-09-27T00:00:04Z","msg":"b1"}
{"ts":"2026-09-27T00:00:05Z","msg":"b2"}
"#,
    )
    .unwrap();
    (app, cur, a, b)
}

fn open_single(app: &mut LogApp, path: &std::path::Path) {
    app.file = Arc::new(LogFile::open(path).unwrap());
    app.has_file = true;
    app.path = path.to_path_buf();
}

/// 同步直驱归并交卷 (注入惯例：不起线程，直接喂 apply)。
fn apply_merge_sync(app: &mut LogApp, paths: Vec<PathBuf>) {
    let out = danqing_log::merge_view::build_merge(
        &paths,
        &[],
        &std::sync::atomic::AtomicBool::new(false),
    );
    app.apply_merge_outcome(out);
}

#[test]
fn merge_switches_workspace_and_preserves_single_state() {
    let (mut app, cur, a, b) = merge_fixture("switch");
    open_single(&mut app, &cur);
    app.selected = 2;
    app.top_row = 1.0;
    app.bookmarks.insert(1);
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone(), b.clone()]);
    assert_eq!(app.workspace, Workspace::Merge);
    let m = app.merge.as_ref().unwrap();
    assert_eq!(m.sources.len(), 3, "当前文件主源 + 两追加");
    assert_eq!(m.row_count(), 9);
    assert_eq!(m.top_row, 0.0);
    // Single 现场冻结原值 (D4 互不丢状态)
    assert_eq!(app.selected, 2);
    assert_eq!(app.top_row, 1.0);
    assert!(app.bookmarks.contains(&1));
    assert_eq!(app.cur_selected(), 0, "访问器读 merge bundle");
    app.update(Msg::ExitMerge);
    assert_eq!(app.workspace, Workspace::Single);
    assert_eq!(app.cur_selected(), 2, "退出后访问器路由回 Single");
    assert!(app.merge.is_some(), "bundle 保留 (再进不重建)");
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

#[test]
fn merge_bookmark_toggle_uses_pack_keys_single_untouched() {
    let (mut app, cur, a, b) = merge_fixture("bm");
    open_single(&mut app, &cur);
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone(), b.clone()]);
    // 时间线：cur0(00) a0(01) b0(02) a1(03) → pos 3 = (源 1, 行 1)
    app.merge.as_mut().unwrap().selected = 3;
    app.toggle_bookmark();
    let m = app.merge.as_ref().unwrap();
    assert!(
        m.bookmarks
            .contains(&danqing_log::merge_view::pack_key(1, 1)),
        "合并书签打在 (源 1, 行 1) pack 键"
    );
    assert!(app.bookmarks.is_empty(), "单文件书签集零触碰 (D4)");
    assert!(app.notice.is_some(), "书签动作有回执");
    app.goto_next_bookmark();
    assert_eq!(
        app.merge.as_ref().unwrap().selected,
        3,
        "只有一个书签 → 环绕回它"
    );
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

#[test]
fn merge_goto_end_routes_to_merge_bundle() {
    let (mut app, cur, a, b) = merge_fixture("nav");
    open_single(&mut app, &cur);
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone(), b.clone()]);
    app.update(Msg::GotoEnd);
    assert_eq!(app.merge.as_ref().unwrap().selected, 8, "9 行 → 末行 8");
    assert_eq!(app.selected, 0, "Single 字段不动 (D4)");
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

#[test]
fn open_file_switches_back_to_single_workspace() {
    let (mut app, cur, a, b) = merge_fixture("openback");
    open_single(&mut app, &cur);
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone(), b.clone()]);
    assert_eq!(app.workspace, Workspace::Merge);
    app.reload_file(a.clone());
    assert_eq!(app.workspace, Workspace::Single, "打开新文件回单文件 (D4)");
    assert!(app.merge.is_some(), "bundle 保留");
    // open_job 在途：测试不拾取，drop 语义收尾 (open.rs)。
    app.open_job = None;
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

#[test]
fn poll_growth_frozen_during_merge_and_resumes_after_exit() {
    let (mut app, cur, a, b) = merge_fixture("tail");
    open_single(&mut app, &cur);
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone(), b.clone()]);
    // 单文件在合并期长大
    {
        use std::io::Write;
        let mut w = std::fs::OpenOptions::new().append(true).open(&cur).unwrap();
        w.write_all(b"2026-09-27T00:00:09Z INFO c3\n").unwrap();
    }
    app.last_stat_poll = Instant::now() - Duration::from_secs(1);
    app.poll_growth();
    assert_eq!(
        app.file.line_count(),
        3,
        "合并期间**单文件侧** tail 冻结 (T7 起合流发生在合并源上，app.file 不动)"
    );
    assert!(app.open_job.is_none());
    app.update(Msg::ExitMerge);
    app.poll_growth();
    // 小追加走同步增量通路 (不开 open_job —— 那是巨量追平的通道)
    assert_eq!(app.file.line_count(), 4, "退出合并后 stat 轮询自然追平");
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

/// T7 翻案 (原 `follow_key_in_merge_notices_instead_of_silent_noop` 告示锁退役):
/// f 在合并态 = 合并跟随 toggle (钉时间线尾), **单文件 follow 不动** (D4)。
#[test]
fn follow_key_in_merge_toggles_merge_follow_only() {
    let (mut app, cur, a, b) = merge_fixture("follow");
    open_single(&mut app, &cur);
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone(), b.clone()]);
    app.toggle_follow();
    assert!(!app.follow, "单文件 follow 不动 (D4)");
    assert!(app.merge.as_ref().unwrap().follow, "合并 follow 接通 (T7)");
    assert_eq!(
        app.merge.as_ref().unwrap().selected,
        app.merge.as_ref().unwrap().row_count() - 1,
        "开跟随即钉时间线尾"
    );
    app.toggle_follow();
    assert!(!app.merge.as_ref().unwrap().follow, "再按解除");
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

/// D4 不丢 (加/减源重建): 旧 bundle 在场的重归并交卷**按路径**搬
/// 书签/显隐/选中 (carry_view_state) —— 减源后序号漂移仍找回，消失源的
/// 键静默丢。首并 (无旧 bundle) 不 carry, 全新状态。
#[test]
fn rebuild_reapply_carries_bookmarks_hidden_and_selection() {
    let (mut app, cur, a, b) = merge_fixture("m-carry");
    open_single(&mut app, &cur);
    // 显隐动作有 merge_gate —— 注付费态 (注入惯例)
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone(), b.clone()]);
    // 书签打在**将被隐藏**的源 1 (键不因隐藏失效，T3 语义);
    // 选中打在可见源 2 (b) —— 掩码索引里找得回才有意义。
    {
        let m = app.merge.as_mut().unwrap();
        m.selected = m.position_of(2, 1).unwrap();
        m.toggle_bookmark(1, 1, 256);
    }
    app.update(Msg::ToggleMergeSource(a.to_string_lossy().into_owned()));
    // 重归并交卷：减掉源 0 (cur) → 旧源 1→新源 0, 旧源 2→新源 1 (序号漂移)
    apply_merge_sync(&mut app, vec![a.clone(), b.clone()]);
    let m = app.merge.as_ref().unwrap();
    assert_eq!(m.sources.len(), 2);
    assert!(m.sources[0].hidden, "显隐按路径继承 (a 仍藏)");
    assert!(
        m.bookmarks
            .contains(&danqing_log::merge_view::pack_key(0, 1)),
        "书签按路径重映射：旧 (1,1) → 新 (0,1)"
    );
    assert_eq!(
        m.selected,
        m.position_of(1, 1).unwrap(),
        "选中行按 (路径，行) 找回：旧 (2,1) → 新 (1,1)"
    );
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

/// D6 门控两态 (两道闸): 免费态入口/三动作全拦 (升级提示 + 零变更);
/// 付费态放行且**永不触发升级提示**。与 session_gate 锁同构。
#[test]
fn merge_gate_blocks_free_tier_and_never_prompts_paid() {
    let (mut app, cur, a, b) = merge_fixture("m-gate");
    open_single(&mut app, &cur);
    // 免费态：入口被拦，弹层不开
    app.update(Msg::OpenMergeMenu);
    assert_eq!(
        app.upgrade_prompt,
        Some(Feature::MergeTimeline),
        "免费态入口 = 升级提示"
    );
    assert!(!app.merge_menu_open, "免费态不得开弹层");
    // 免费态：动作兜底闸全拦 (零变更)
    app.update(Msg::MergeSourcePicked(a.clone()));
    assert!(app.merge.is_none(), "免费态加源零起并");
    app.update(Msg::RemoveSelectedMergeSource);
    app.update(Msg::ToggleMergeSource(a.to_string_lossy().into_owned()));
    assert!(app.merge.is_none(), "免费态动作零变更");
    // 付费态：放行; 全程永不触发升级提示
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    app.upgrade_prompt = None;
    app.update(Msg::OpenMergeMenu);
    assert!(app.merge_menu_open, "付费态开弹层");
    assert!(app.upgrade_prompt.is_none(), "付费态永不误弹");
    app.update(Msg::MergeSourcePicked(a.clone()));
    assert!(app.upgrade_prompt.is_none());
    // 起并走 AsyncJob: 测试不拾取，drop 语义收尾 (open.rs 同规)
    app.merge_job = Default::default();
    // 直注交卷验证付费动作真生效 (注入惯例)
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone(), b.clone()]);
    app.update(Msg::ToggleMergeSource(a.to_string_lossy().into_owned()));
    assert!(
        app.merge.as_ref().unwrap().sources[1].hidden,
        "付费态显隐真切换"
    );
    assert!(app.upgrade_prompt.is_none(), "动作后仍不误弹");
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

// ---- 三连接门 (todo-gate-trio G2/G3/G4 两态锁) ----

/// T5 应用层 (腿 D): 快捷档/手输走同一 edit_source_time 通路 ——
/// 作用**选中**源，解析边界重提 (ts 变，文件行索引不动), 门控两态。
#[test]
fn merge_time_edit_targets_selected_and_reextracts() {
    let (mut app, cur, a, b) = merge_fixture("m-time");
    open_single(&mut app, &cur);
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone(), b.clone()]);
    // 无选中 → 提示零变更
    app.update(Msg::NudgeMergeOffset(-1_000));
    assert!(app.merge.as_ref().unwrap().sources[0].offset_ms == 0);
    // 选中源 1, 快捷档 -1s
    app.merge_source_selected = Some(a.to_string_lossy().into_owned());
    app.update(Msg::NudgeMergeOffset(-1_000));
    let m = app.merge.as_ref().unwrap();
    assert_eq!(m.sources[1].offset_ms, -1_000, "快捷档落选中源");
    assert_eq!(m.sources[0].offset_ms, 0, "别源零触碰");
    // 手输偏移 (绝对值) + 时区
    app.update(Msg::SetMergeOffset("-7000".into()));
    app.update(Msg::SetMergeTz("+08:00".into()));
    let m = app.merge.as_ref().unwrap();
    assert_eq!(m.sources[1].offset_ms, -7_000);
    assert_eq!(m.sources[1].tz_offset_ms, 8 * 3_600_000);
    // 拒收明示 (非数)
    app.update(Msg::SetMergeOffset("abc".into()));
    assert!(app.notice.as_ref().is_some_and(|(t, _)| t.contains("毫秒")));
    // 免费态：门拦零变更 (两道闸)
    app.entitlement = Entitlement::Free;
    app.upgrade_prompt = None;
    app.update(Msg::NudgeMergeOffset(1_000));
    assert_eq!(
        app.upgrade_prompt,
        Some(Feature::MergeTimeline),
        "免费态动作兜底闸"
    );
    assert_eq!(app.merge.as_ref().unwrap().sources[1].offset_ms, -7_000);
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

/// G2 列配置两态：免费态入口/手势动作全拦 (升级提示 + 零变更 + 不落盘);
/// 付费态放行永不误弹。列配置段「不读不写」另有通路锁。
#[test]
fn column_gate_blocks_free_tier_and_never_prompts_paid() {
    let cfg = temp_cfg_path("g2");
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    let p = temp_log(
        b"{\"a\":1}
{\"a\":2}
{\"a\":3}
",
    );
    app.file = Arc::new(LogFile::open(&p).unwrap());
    app.has_file = true;
    app.path = std::path::PathBuf::from("C:/logs/g2.jsonl");
    // 铺 schema (set_width 只收已知列名)
    app.schema = Some(Arc::new(danqing_log::jsonl::Schema {
        columns: vec![danqing_log::jsonl::Column {
            name: "a".into(),
            width_chars: 8,
        }],
    }));
    // 免费态：入口拦
    app.update(Msg::OpenColMenu);
    assert_eq!(app.upgrade_prompt, Some(Feature::ColumnConfig));
    assert!(!app.col_menu_open);
    // 免费态：三动作兜底闸全拦 (列摆法零变更)
    app.update(Msg::ColumnWidthSet("a".into(), 250.0));
    app.update(Msg::ColumnWidthClear("a".into()));
    app.update(Msg::ColumnMoveBefore("a".into(), None));
    assert!(app.columns.widths.is_empty(), "免费态列摆法零变更");
    // 付费态：放行且永不误弹
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    app.upgrade_prompt = None;
    app.update(Msg::OpenColMenu);
    assert!(app.col_menu_open && app.upgrade_prompt.is_none());
    app.update(Msg::CloseColMenu);
    app.update(Msg::ColumnWidthSet("a".into(), 250.0));
    assert_eq!(
        app.columns.widths.get("a"),
        Some(&250.0),
        "付费态动作真生效"
    );
    assert!(app.upgrade_prompt.is_none());
    std::fs::remove_file(&cfg).ok();
    std::fs::remove_file(&p).ok();
}

/// G2 通路段「不读不写」: 免费期运行态不上账; **付费期已写的保留不删**
/// (数据永在 —— 降级不毁数据)。
#[test]
fn column_config_segment_skips_read_write_in_free_but_preserves_paid() {
    let cfg = temp_cfg_path("g2-seg");
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    let p = temp_log(
        b"{\"a\":1}
{\"a\":2}
{\"a\":3}
",
    );
    app.file = Arc::new(LogFile::open(&p).unwrap());
    app.has_file = true;
    app.path = std::path::PathBuf::from("C:/logs/seg.jsonl");
    app.schema = Some(Arc::new(danqing_log::jsonl::Schema {
        columns: vec![danqing_log::jsonl::Column {
            name: "a".into(),
            width_chars: 8,
        }],
    }));
    // 付费期写入列宽
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    app.update(Msg::ColumnWidthSet("a".into(), 300.0));
    assert!(app.save_state());
    // 降级免费：不读 (默认摆法), 不写 (磁盘原值保留)
    app.entitlement = Entitlement::Free;
    app.columns.widths.clear();
    app.load_state_for_current_file();
    assert!(app.columns.widths.is_empty(), "免费态不读列配置段");
    app.save_state(); // 免费态落盘：付费数据须原样保留
    // 再回付费：付费期写的还在 (没被免费期 save 抹掉)
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    app.load_state_for_current_file();
    assert_eq!(
        app.columns.widths.get("a"),
        Some(&300.0),
        "免费期 save 不毁付费期列配置"
    );
    std::fs::remove_file(&cfg).ok();
    std::fs::remove_file(&p).ok();
}

/// G3 书签持久化两态：会话内书签照用 (toggle 不拦不弹窗，v1.0 行为);
/// 免费态不跨重启恢复; 付费期已写的免费期不删。
#[test]
fn bookmark_persist_gated_but_in_session_free() {
    let cfg = temp_cfg_path("g3");
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    let p = temp_log(
        b"l0
l1
l2
",
    );
    app.file = Arc::new(LogFile::open(&p).unwrap());
    app.has_file = true;
    app.path = std::path::PathBuf::from("C:/logs/g3.log");
    // 免费态：会话内书签照用 —— toggle 不弹窗 (提示位在许可页，不在这)
    app.update(Msg::Noop); // 起步
    app.bookmarks.insert(1);
    app.upgrade_prompt = None;
    assert!(app.bookmarks.contains(&1), "会话内书签照用");
    assert!(app.upgrade_prompt.is_none(), "toggle 不弹窗 (G3 裁决)");
    // 付费期写书签 → 降级：免费期不读不写，付费数据保留
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    app.bookmarks.insert(2);
    assert!(app.save_state());
    app.entitlement = Entitlement::Free;
    app.bookmarks.clear();
    app.load_state_for_current_file();
    assert!(app.bookmarks.is_empty(), "免费态不跨重启恢复");
    app.save_state();
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    app.load_state_for_current_file();
    assert!(app.bookmarks.contains(&2), "免费期 save 不毁付费期书签");
    std::fs::remove_file(&cfg).ok();
    std::fs::remove_file(&p).ok();
}

/// G4 字段点选两态：免费态「字段…」拦 (升级提示，弹层不开);
/// 付费态放行永不误弹。手输迷你语法不经此门 (照用)。
#[test]
fn picker_gate_blocks_free_tier_and_never_prompts_paid() {
    let cfg = temp_cfg_path("g4");
    let mut app = LogApp::new_empty_at(Some(cfg.clone()));
    let p = temp_log(
        b"{\"a\":1}
{\"a\":2}
{\"a\":3}
",
    );
    app.file = Arc::new(LogFile::open(&p).unwrap());
    app.has_file = true;
    app.path = std::path::PathBuf::from("C:/logs/g4.jsonl");
    app.update(Msg::OpenPicker);
    assert_eq!(app.upgrade_prompt, Some(Feature::FieldPicker));
    assert!(!app.picker_open, "免费态不开弹层");
    // 付费态放行且永不误弹
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    app.upgrade_prompt = None;
    app.update(Msg::OpenPicker);
    assert!(app.picker_open && app.upgrade_prompt.is_none());
    std::fs::remove_file(&cfg).ok();
    std::fs::remove_file(&p).ok();
}

/// 上限拒绝 (D7): 已 8 源时加第 9 个 = 明示拒绝 + 零变更。
#[test]
fn merge_add_source_rejects_ninth_with_notice() {
    let (mut app, cur, a, b) = merge_fixture("m-cap");
    open_single(&mut app, &cur);
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone(), b.clone()]);
    // 灌满：追加到 8 源 (直接改 sources 不可行 —— 走 add 的去重/上限逻辑前先造满)
    let m = app.merge.as_mut().unwrap();
    for i in 0..5 {
        m.sources.push(danqing_log::merge_view::MergeSource {
            path: std::env::temp_dir().join(format!("filler-{i}.log")),
            file: Arc::new(LogFile::open(&cur).unwrap()),
            schema: None,
            route: danqing_log::timestamp::TsRoute::LogPrefix(
                danqing_log::timestamp::TsFormat::Iso8601,
            ),
            offset_ms: 0,
            tz_offset_ms: 0,
            hidden: false,
            stale: false,
        });
    }
    assert_eq!(m.sources.len(), 8, "灌满至上限");
    app.update(Msg::MergeSourcePicked(
        std::env::temp_dir().join("ninth.log"),
    ));
    assert_eq!(
        app.merge.as_ref().unwrap().sources.len(),
        8,
        "第 9 个拒绝加入 (D7)"
    );
    assert!(
        app.notice
            .as_ref()
            .is_some_and(|(t, k)| t.contains("上限") && matches!(k, crate::NoticeKind::Warn)),
        "明示上限文案"
    );
    // 去重拒绝 (明示)
    app.update(Msg::MergeSourcePicked(a.clone()));
    assert!(
        app.notice
            .as_ref()
            .is_some_and(|(t, _)| t.contains("已在合并")),
        "重复源明示"
    );
    assert_eq!(app.merge.as_ref().unwrap().sources.len(), 8);
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

// ---- 腿 E (T6): req_id 追踪 ----

/// T6 夹具：在 merge_fixture 上覆写 cur/b —— cur.log 行 1 含 req_id=aaa111,
/// b.jsonl 行 1 同值 (且 msg 带空格，供字段值放大验), a.log 无命中。
fn trace_fixture(tag: &str) -> (LogApp, PathBuf, PathBuf, PathBuf) {
    let (app, cur, a, b) = merge_fixture(tag);
    std::fs::write(
            &cur,
            b"2026-09-27T00:00:00Z INFO boot\n2026-09-27T00:00:02Z INFO request done req_id=aaa111\n2026-09-27T00:00:04Z INFO idle\n",
        )
        .unwrap();
    std::fs::write(
        &b,
        br#"{"ts":"2026-09-27T00:00:01.500Z","msg":"enter"}
{"ts":"2026-09-27T00:00:02.500Z","msg":"request timeout","req_id":"aaa111"}
{"ts":"2026-09-27T00:00:06.000Z","msg":"end"}
"#,
    )
    .unwrap();
    (app, cur, a, b)
}

/// 泵追踪作业至落地 (真线程，小文件瞬时完成; 有界自旋防挂死)。
fn pump_trace(app: &mut LogApp) {
    for _ in 0..1000 {
        if let Some(out) = app.trace_job.poll() {
            app.apply_trace_outcome(out);
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("追踪作业 1s 内未完成");
}

/// 端到端：.log 源选区子串追踪 → 三源同请求行一次滤出 → 锚定回发起行 →
/// 底栏常驻追踪态 → ClearTrace 回全量 (验收 b 的机器半边)。
#[test]
fn trace_value_filters_across_sources_and_anchors() {
    let (mut app, cur, a, b) = trace_fixture("chain");
    open_single(&mut app, &cur);
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone(), b.clone()]);
    assert_eq!(app.merge.as_ref().unwrap().row_count(), 9);
    // 从 .log 源 (源 0) 行 1 追踪子串 "aaa111" (选区字节直注，鼠标路径在 view 锁)
    let text = danqing_log::merge_view::row_text(&app.merge.as_ref().unwrap().sources[0].file, 1);
    let lo = text.find("aaa111").unwrap();
    app.update(Msg::TraceValue {
        src: 0,
        line: 1,
        lo,
        hi: lo + 6,
    });
    pump_trace(&mut app);
    let m = app.merge.as_ref().unwrap();
    assert_eq!(m.trace.as_deref(), Some("aaa111"), "追踪值落账");
    assert_eq!(m.row_count(), 2, "三源同请求行一次滤出");
    let seq: Vec<(u32, u32)> = (0..m.row_count())
        .map(|p| m.row_at(p).map(|r| (r.src, r.line)).unwrap())
        .collect();
    assert_eq!(seq, vec![(0, 1), (2, 1)], "命中集按时间线序");
    assert_eq!(
        m.row_at(m.selected).map(|r| (r.src, r.line)),
        Some((0, 1)),
        "选中锚回发起行"
    );
    assert!(
        app.status.contains("追踪 \"aaa111\""),
        "底栏常驻追踪态：{}",
        app.status
    );
    app.update(Msg::ClearTrace);
    let m = app.merge.as_ref().unwrap();
    assert_eq!(m.row_count(), 9, "清除回全量");
    assert!(m.trace.is_none());
    assert_eq!(
        m.row_at(m.selected).map(|r| (r.src, r.line)),
        Some((0, 1)),
        "清除后选中锚行不漂"
    );
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

/// JSONL 行「取字段值」: 双击落在带空格值内部 → 放大整个字段值
/// (选区只圈 "timeout", 追踪值 = "request timeout")。
#[test]
fn trace_on_jsonl_row_snaps_to_full_field_value() {
    let (mut app, cur, a, b) = trace_fixture("snap");
    open_single(&mut app, &cur);
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone(), b.clone()]);
    let text = danqing_log::merge_view::row_text(&app.merge.as_ref().unwrap().sources[2].file, 1);
    let lo = text.find("timeout").unwrap();
    app.update(Msg::TraceValue {
        src: 2,
        line: 1,
        lo,
        hi: lo + 7,
    });
    pump_trace(&mut app);
    let m = app.merge.as_ref().unwrap();
    assert_eq!(
        m.trace.as_deref(),
        Some("request timeout"),
        "字段值放大 (含空格全值; 摘掉 snap 此断言红)"
    );
    assert_eq!(m.row_count(), 1, "全值作一条 Bare 字面量命中本行");
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

/// R5 族：在途追踪遇显隐切换 (掩码重建) → 代次作废，晚到不复活。
#[test]
fn trace_in_flight_invalidated_on_source_toggle() {
    let (mut app, cur, a, b) = trace_fixture("stale");
    open_single(&mut app, &cur);
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone(), b.clone()]);
    let text = danqing_log::merge_view::row_text(&app.merge.as_ref().unwrap().sources[0].file, 1);
    let lo = text.find("aaa111").unwrap();
    app.update(Msg::TraceValue {
        src: 0,
        line: 1,
        lo,
        hi: lo + 6,
    });
    // 不泵，直接切显隐 —— 掩码重建作废过滤行集，在途追踪同作废
    app.update(Msg::ToggleMergeSource(a.to_string_lossy().into_owned()));
    // 注 (评审 Nit): 本锁管的是**接线** (该路径确实调了 invalidate);
    // 「已完成但仍被丢弃」那一半由 search.rs 的
    // `async_job_invalidate_discards_inflight_result` 用确定性子锁住 ——
    // 这里不做「睡够久再断言」的时序猜测 (家法：测试不许时序侥幸)。
    for _ in 0..200 {
        assert!(app.trace_job.poll().is_none(), "作废旧轮的晚到结果不得拾取");
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let m = app.merge.as_ref().unwrap();
    assert!(
        m.filtered.is_none() && m.trace.is_none(),
        "重建后无追踪残留 (filter_landed 同族纪律)"
    );
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

/// Ctrl+R 单文件态 → 出声指路合并视图 (P24); 合并态未持焦 → 指路选值。
#[test]
fn ctrl_r_outside_merge_selection_says_why() {
    let (mut app, cur, a, b) = trace_fixture("ctrl-r");
    open_single(&mut app, &cur);
    press_ctrl(&mut app, "r");
    assert!(
        app.notice
            .as_ref()
            .is_some_and(|(t, _)| t.contains("合并视图")),
        "单文件态 Ctrl+R 说清为什么：{:?}",
        app.notice
    );
    // 合并态 (LogView 未持焦 → 键到应用层): 指路先选值
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone(), b.clone()]);
    app.notice = None;
    press_ctrl(&mut app, "r");
    assert!(
        app.notice
            .as_ref()
            .is_some_and(|(t, _)| t.contains("追踪值")),
        "合并态未持焦指路选值：{:?}",
        app.notice
    );
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

// ---- 腿 F (T7): live-tail 合流 ----

/// 泵归并作业至落地 (真线程，小文件瞬时; 有界自旋防挂死)。
fn pump_merge(app: &mut LogApp) {
    for _ in 0..1000 {
        app.pickup_merge_job();
        if !app.merge_job_live {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("归并作业 1s 内未完成");
}

/// per-source 增量合流端到端：合并期间某源长大 → 合并时间线收新行，
/// **单文件侧保持冻结** (app.file 不动，退出后自然追平 —— D4)。
#[test]
fn poll_growth_merge_appends_per_source_single_side_frozen() {
    let (mut app, cur, a, b) = merge_fixture("mrg-tail");
    open_single(&mut app, &cur);
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone(), b.clone()]);
    assert_eq!(app.merge.as_ref().unwrap().row_count(), 9);
    {
        use std::io::Write;
        let mut w = std::fs::OpenOptions::new().append(true).open(&cur).unwrap();
        w.write_all(b"2026-09-27T00:00:09Z INFO c3\n").unwrap();
    }
    app.poll_growth();
    let m = app.merge.as_ref().unwrap();
    assert_eq!(m.row_count(), 10, "合并时间线收新行 (源 0 行 3)");
    let last = m.row_at(9).unwrap();
    assert_eq!((last.src, last.line), (0, 3), "新行在时间线尾");
    assert_eq!(app.file.line_count(), 3, "单文件侧仍冻结 (退出才追平)");
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

/// 断流降级：源文件消失 → 标记 + 底栏「断流 N 源」, 其余源照常合流;
/// 恢复可读 (stat 成功) 即清标记。
#[test]
fn poll_growth_merge_marks_stale_and_recovers() {
    let (mut app, cur, a, b) = merge_fixture("mrg-stale");
    open_single(&mut app, &cur);
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone(), b.clone()]);
    std::fs::remove_file(&a).unwrap(); // 源 1 消失 (mmap 持有仍可读旧快照)
    app.poll_growth();
    {
        let m = app.merge.as_ref().unwrap();
        assert!(m.sources[1].stale, "消失的源标断流");
        assert!(!m.sources[0].stale && !m.sources[2].stale, "其余源不连坐");
    }
    assert!(app.status.contains("断流 1 源"), "底栏明示：{}", app.status);
    // 断流期间其余源照常合流 (不拖垮全局)
    {
        use std::io::Write;
        let mut w = std::fs::OpenOptions::new().append(true).open(&cur).unwrap();
        w.write_all(b"2026-09-27T00:00:09Z INFO c3\n").unwrap();
    }
    app.poll_growth();
    assert_eq!(
        app.merge.as_ref().unwrap().row_count(),
        10,
        "断流源在场，其他源照常合流"
    );
    // 恢复可读即清 (注入断流标记到**健在**的源 0 —— 源 1 已被删，stat 只会
    // 继续失败; Windows 上被 mmap 持有的文件删除后处于 delete-pending,
    // 同名重建被拒直到句柄关闭，真轮转走改名 + 新建):
    app.merge.as_mut().unwrap().sources[0].stale = true;
    app.poll_growth();
    assert!(
        !app.merge.as_ref().unwrap().sources[0].stale,
        "恢复可读自清 (内容比对管后续)"
    );
    assert!(
        app.merge.as_ref().unwrap().sources[1].stale,
        "源 1 仍断流 (文件没回来)"
    );
    assert!(!app.merge_job_live, "无变化不触发重归并");
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&b).ok();
}

/// 轮转 (head 变): 全量重归并 (worker), 旧 bundle 保持可见至交卷;
/// 在途期间轮询不叠加 (merge_job_live 门禁)。
#[test]
fn poll_growth_merge_rotation_rebuilds_via_worker() {
    let (mut app, cur, a, b) = merge_fixture("mrg-rot");
    open_single(&mut app, &cur);
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone(), b.clone()]);
    // 源 1 轮转：**改名 + 新建** (真轮转形态 —— 被 mmap 持有的文件不能
    // 就地覆写，Windows ERROR_USER_MAPPED_FILE; 改名靠 DELETE 共享走得通)
    let rotated = a.with_extension("old");
    std::fs::rename(&a, &rotated).unwrap();
    std::fs::write(
            &a,
            b"2026-09-27T01:00:00Z INFO r0\n2026-09-27T01:00:01Z INFO r1\n2026-09-27T01:00:02Z INFO r2\n",
        )
        .unwrap();
    app.poll_growth();
    assert!(app.merge_job_live, "轮转 → 重归并作业在途");
    assert!(
        app.notice
            .as_ref()
            .is_some_and(|(t, k)| t.contains("轮转") && matches!(k, crate::NoticeKind::Warn)),
        "轮转明示：{:?}",
        app.notice
    );
    // 在途期间再 poll 不叠加 (旧 bundle 行数没变 = 旧内容保持可见)
    app.poll_growth();
    assert_eq!(
        app.merge.as_ref().unwrap().row_count(),
        9,
        "交卷前旧 bundle 在"
    );
    pump_merge(&mut app);
    assert_eq!(
        app.merge.as_ref().unwrap().sources[1].file.line_count(),
        3,
        "轮转源新内容落地"
    );
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&rotated).ok();
    std::fs::remove_file(&b).ok();
}

/// 合并跟随 (T7): f 键接通 —— 开即钉时间线尾，新行合流保持钉尾。
#[test]
fn merge_follow_pins_timeline_tail() {
    let (mut app, cur, a, b) = merge_fixture("mrg-follow");
    open_single(&mut app, &cur);
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone(), b.clone()]);
    app.merge.as_mut().unwrap().selected = 0;
    press(&mut app, Key::Character("f".to_string()));
    let m = app.merge.as_ref().unwrap();
    assert!(m.follow, "f 键接通合并跟随 (T3 告示退役)");
    assert_eq!(m.selected, 8, "开跟随即钉尾");
    {
        use std::io::Write;
        let mut w = std::fs::OpenOptions::new().append(true).open(&cur).unwrap();
        w.write_all(b"2026-09-27T00:00:09Z INFO c3\n").unwrap();
    }
    app.poll_growth();
    let m = app.merge.as_ref().unwrap();
    assert_eq!(m.row_count(), 10);
    assert_eq!(m.selected, 9, "新行合流保持钉尾");
    press(&mut app, Key::Character("f".to_string()));
    assert!(!app.merge.as_ref().unwrap().follow, "再按解除");
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

/// 弹层家族规矩 (T4): 互斥入 close_popovers/popover_open + Esc 插层
/// (升级提示 > 设置卡 > **合并源管理** > 命名会话 > …)。
#[test]
fn merge_menu_joins_popover_family_with_esc_insertion() {
    let (mut app, cur, a, b) = merge_fixture("m-family");
    open_single(&mut app, &cur);
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    // 互斥：开二关一
    app.update(Msg::OpenSessionMenu);
    app.update(Msg::OpenMergeMenu);
    assert!(app.merge_menu_open && !app.session_menu_open, "开二关一");
    assert!(app.popover_open(), "家族判据纳入 merge_menu");
    // Esc 插层：合并源管理在命名会话**前**被关
    let esc = Event::Key {
        key: Key::Named(NamedKey::Escape),
        pressed: true,
        shift: false,
        ctrl: false,
        alt: false,
    };
    assert!(matches!(
        app.app_key_filter(&esc),
        Some(Msg::CloseMergeMenu)
    ));
    app.update(Msg::CloseMergeMenu);
    assert!(!app.merge_menu_open);
    assert!(app.merge_source_selected.is_none(), "关清草稿 (指针)");
    // 设置卡优先于合并源管理
    app.update(Msg::OpenMergeMenu);
    app.update(Msg::OpenSettings);
    // OpenSettings 走 close_popovers: 弹层先被关 (互斥), 无并存
    assert!(
        app.settings_open && !app.merge_menu_open,
        "设置卡与弹层互斥"
    );
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

#[test]
fn start_merge_refuses_bad_arity_and_over_cap() {
    let (mut app, cur, a, b) = merge_fixture("arity");
    // 无文件 + 零追加 = 不够两源
    app.update(Msg::StartMerge(vec![]));
    assert!(app.merge.is_none(), "不够两源不启动");
    assert!(app.notice.is_some());
    // 9 个追加源 > 上限 8 (无当前文件)
    let many: Vec<PathBuf> = (0..9)
        .map(|i| std::env::temp_dir().join(format!("nope-{i}.log")))
        .collect();
    app.update(Msg::StartMerge(many));
    assert!(app.merge.is_none(), "超上限拒绝");
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

// ---- T8: sessions 载荷 merge group (SPEC-v1x-merge-timeline D4) ----

/// T8 载荷简写。
fn mss(
    p: &std::path::Path,
    off: i64,
    tz: i64,
    hidden: bool,
) -> danqing_log::merge_view::MergeSourceState {
    danqing_log::merge_view::MergeSourceState {
        path: p.to_string_lossy().into_owned(),
        offset_ms: off,
        tz_offset_ms: tz,
        hidden,
    }
}

/// 保存侧：合并工作区存会话 = 单文件侧四样 + 合并组快照 (源/偏移/显隐),
/// 落盘读回逐项对得上。
#[test]
fn save_session_in_merge_captures_merge_group() {
    let cfg = temp_cfg_path("t8-save");
    let (mut app, cur, a, b) = merge_fixture("t8-save");
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    open_single(&mut app, &cur);
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone(), b.clone()]);
    // 造可辨现场：源 1 拨 -3s (tz 不动), 源 2 隐藏
    {
        let tz = app.merge.as_ref().unwrap().sources[1].tz_offset_ms;
        let m = app.merge.as_mut().unwrap();
        m.set_time_params(1, -3000, tz);
        m.sources[2].hidden = true;
        m.rebuild_masked();
    }
    app.update(Msg::SaveSession("事故台".into()));
    assert_eq!(app.sessions.len(), 1);
    let g = app.sessions[0]
        .merge
        .as_ref()
        .expect("合并态保存须带 merge 段");
    assert_eq!(g.sources.len(), 3);
    assert_eq!(g.sources[0].path, cur.to_string_lossy());
    assert_eq!(g.sources[1].offset_ms, -3000);
    assert!(g.sources[2].hidden);
    // 单文件侧载荷仍来自冻结现场 (filter 串是单文件侧的)
    assert_eq!(app.sessions[0].path, cur.to_string_lossy());
    // 落盘 → 读回 (账本面 roundtrip 一并实证)
    let saved = danqing_log::columns::ColumnFiles::load_from(&cfg.with_extension("state.json"));
    let g2 = saved.sessions_for_path(&cur.to_string_lossy())[0]
        .merge
        .clone()
        .expect("落盘后 merge 段存活");
    assert_eq!(g2.sources[1].offset_ms, -3000);
    assert!(g2.sources[2].hidden);
    std::fs::remove_file(cfg.with_extension("state.json")).ok();
    std::fs::remove_file(&cfg).ok();
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

/// 恢复侧端到端：合并中存 → 退出合并 + bundle 丢弃 (「明天重开」) →
/// 应用会话 → 真线程归并落地 → 源组/偏移/显隐全回 (排序与行集双证)。
#[test]
fn apply_merge_session_restores_group_offsets_and_hidden() {
    let cfg = temp_cfg_path("t8-apply");
    let (mut app, cur, a, b) = merge_fixture("t8-apply");
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    open_single(&mut app, &cur);
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone(), b.clone()]);
    {
        let tz = app.merge.as_ref().unwrap().sources[1].tz_offset_ms;
        let m = app.merge.as_mut().unwrap();
        m.set_time_params(1, -3000, tz);
        m.sources[2].hidden = true;
        m.rebuild_masked();
    }
    app.update(Msg::SaveSession("事故台".into()));
    // 明天重开 = 退出合并且 bundle 不在 (会话是唯一记忆)
    app.update(Msg::ExitMerge);
    app.merge = None;
    app.update(Msg::ApplySession("事故台".into()));
    assert!(app.merge_job_live, "合并组会话应用 = 后台归并在途");
    pump_merge(&mut app);
    assert_eq!(app.workspace, Workspace::Merge, "交卷后进合并工作区");
    let m = app.merge.as_ref().unwrap();
    assert_eq!(m.sources.len(), 3, "源组回来");
    assert_eq!(m.sources[1].offset_ms, -3000, "保存偏移套回");
    assert!(m.sources[2].hidden, "保存显隐套回");
    assert_eq!(m.row_count(), 6, "隐藏源行不进索引 (3+3)");
    let r0 = m.row_at(0).unwrap();
    assert_eq!(
        (r0.src, r0.line),
        (1, 0),
        "偏移生效：a0@01s-3s=前日 23:59:58 排最前"
    );
    assert_eq!(
        app.session_selected.as_deref(),
        Some("事故台"),
        "应用记选中 (删除指针)"
    );
    std::fs::remove_file(cfg.with_extension("state.json")).ok();
    std::fs::remove_file(&cfg).ok();
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

/// 源缺失**明示跳过**不拒全体 (T8 验收 g): 三源缺一 → 两源重建，
/// notice 说清跳过几个。
#[test]
fn apply_merge_session_skips_missing_sources_with_notice() {
    let cfg = temp_cfg_path("t8-skip");
    let (mut app, cur, a, b) = merge_fixture("t8-skip");
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    open_single(&mut app, &cur);
    let ghost = std::env::temp_dir().join("danqing-mt-t8-skip-ghost.log");
    std::fs::remove_file(&ghost).ok(); // 保证不存在
    let local_tz = danqing_log::merge_view::local_tz_offset_ms();
    app.sessions.push(danqing_log::columns::SessionEntry {
        path: cur.to_string_lossy().into_owned(),
        name: "残缺台".into(),
        filter: String::new(),
        search: String::new(),
        config: danqing_log::columns::ColumnConfig::default(),
        expands: Vec::new(),
        merge: Some(danqing_log::merge_view::MergeGroup {
            sources: vec![
                mss(&cur, 0, local_tz, false),
                mss(&a, 0, local_tz, false),
                mss(&ghost, 0, local_tz, false),
            ],
        }),
        updated: 1,
    });
    app.update(Msg::ApplySession("残缺台".into()));
    let note = app
        .notice
        .as_ref()
        .map(|(t, _)| t.clone())
        .unwrap_or_default();
    assert!(note.contains("跳过 1 个缺失源"), "缺失明示，实得：{note}");
    pump_merge(&mut app);
    let m = app.merge.as_ref().unwrap();
    assert_eq!(m.sources.len(), 2, "缺失源不进组");
    assert_eq!(m.row_count(), 6, "两源行集 (3+3)");
    std::fs::remove_file(cfg.with_extension("state.json")).ok();
    std::fs::remove_file(&cfg).ok();
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

/// 现存源不足两个 = 整体拒绝并说清 (零副作用：工作区不动/不起作业/
/// 会话留着); 合并现场不被误伤。
#[test]
fn apply_merge_session_refuses_when_fewer_than_two_sources_survive() {
    let cfg = temp_cfg_path("t8-refuse");
    let (mut app, cur, a, b) = merge_fixture("t8-refuse");
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    open_single(&mut app, &cur);
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone(), b.clone()]);
    let ghost1 = std::env::temp_dir().join("danqing-mt-t8-refuse-g1.log");
    let ghost2 = std::env::temp_dir().join("danqing-mt-t8-refuse-g2.log");
    std::fs::remove_file(&ghost1).ok();
    std::fs::remove_file(&ghost2).ok();
    app.sessions.push(danqing_log::columns::SessionEntry {
        path: cur.to_string_lossy().into_owned(),
        name: "全缺台".into(),
        filter: String::new(),
        search: String::new(),
        config: danqing_log::columns::ColumnConfig::default(),
        expands: Vec::new(),
        merge: Some(danqing_log::merge_view::MergeGroup {
            sources: vec![mss(&ghost1, 0, 0, false), mss(&ghost2, 0, 0, false)],
        }),
        updated: 1,
    });
    app.update(Msg::ApplySession("全缺台".into()));
    assert_eq!(app.workspace, Workspace::Merge, "拒绝不动工作区");
    assert!(!app.merge_job_live, "拒绝不起归并作业");
    assert!(app.pending_merge_apply.is_none(), "拒绝不挂载荷");
    let note = app
        .notice
        .as_ref()
        .map(|(t, _)| t.clone())
        .unwrap_or_default();
    assert!(note.contains("不足两个"), "说清为何拒，实得：{note}");
    assert!(
        app.sessions.iter().any(|s| s.name == "全缺台"),
        "会话留着 (修源后重试)"
    );
    std::fs::remove_file(cfg.with_extension("state.json")).ok();
    std::fs::remove_file(&cfg).ok();
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

/// pickup **交付才清**在途标记 (T8 修 T7 遗留错形): 在途 poll 空转
/// **不清** live —— 「重归并在途不叠加」门禁不许提前开门 (旧
/// 「先清后 poll」形让 pump 首拾取即返回，作业还在飞)。慢作业
/// (50ms) 保证在途窗口确定存在，无时序侥幸。
#[test]
fn pickup_merge_job_keeps_live_flag_until_delivery() {
    let (mut app, cur, a, b) = merge_fixture("t8-live");
    open_single(&mut app, &cur);
    app.merge_job_live = true;
    let paths = vec![cur.clone(), a.clone(), b.clone()];
    app.merge_job.launch(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        danqing_log::merge_view::build_merge(
            &paths,
            &[],
            &std::sync::atomic::AtomicBool::new(false),
        )
    });
    app.pickup_merge_job(); // 在途空转
    assert!(app.merge_job_live, "在途 poll 空转不清 live 标记");
    assert!(app.merge.is_none(), "未交付不换入");
    pump_merge(&mut app);
    assert!(!app.merge_job_live, "交付后清");
    assert_eq!(app.merge.as_ref().unwrap().row_count(), 9);
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

/// 评审 C1 端到端 (安全审计 Required 同源): 换源落地点 (apply_merge_outcome)
/// 之后，**旧源集合的在途追踪结果**迟到 —— 必须丢弃 + 出声，不许越界 panic
/// (release 档 = 整进程 abort)。
#[test]
fn late_trace_after_source_swap_is_discarded_with_notice() {
    use danqing_log::merge_view::TraceOutcome;
    let (mut app, cur, a, b) = merge_fixture("t9-c1");
    open_single(&mut app, &cur);
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone(), b.clone()]);
    assert_eq!(app.merge.as_ref().unwrap().sources.len(), 3);
    // 三源时代发起的追踪交卷 (命中表 3 条) —— 但期间源集合换成了 2 源
    let stale = TraceOutcome {
        hits: vec![vec![0], vec![0], vec![0]],
        scanned: vec![3, 3, 3],
        value: "req".into(),
        anchor: (0, 0),
        elapsed: std::time::Duration::ZERO,
    };
    app.merge = None; // 「明天重开」: 换一组源
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone()]);
    assert_eq!(app.merge.as_ref().unwrap().sources.len(), 2);
    app.apply_trace_outcome(stale); // 迟到交卷：不许崩
    let note = app
        .notice
        .as_ref()
        .map(|(t, _)| t.clone())
        .unwrap_or_default();
    assert!(note.contains("源已变化"), "出声说清，实得：{note}");
    assert!(app.merge.as_ref().unwrap().trace.is_none(), "追踪未落地");
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

/// 评审 Optional 9: 「重启后**没有单文件在手**」直接应用合并组会话 ——
/// 单文件侧载荷跑在空现场上 (无文件/无 schema), 不许炸、不许半途留下
/// 半套状态; 合并组照常重建。
#[test]
fn merge_session_restore_without_open_file_is_safe() {
    let cfg = temp_cfg_path("t9-nofile");
    let (mut app, cur, a, b) = merge_fixture("t9-nofile");
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    open_single(&mut app, &cur);
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone(), b.clone()]);
    app.update(Msg::SaveSession("冷启台".into()));
    // 冷启：无文件在手 (has_file = false, file = 空文件占位)
    app.has_file = false;
    app.file = Arc::new(LogFile::open(&cur).unwrap());
    app.merge = None;
    app.update(Msg::ApplySession("冷启台".into()));
    assert!(app.merge_job_live, "合并组照常起归并");
    pump_merge(&mut app);
    let m = app.merge.as_ref().unwrap();
    assert_eq!(m.sources.len(), 3, "源组重建");
    assert_eq!(m.row_count(), 9);
    assert_eq!(app.workspace, Workspace::Merge);
    std::fs::remove_file(cfg.with_extension("state.json")).ok();
    std::fs::remove_file(&cfg).ok();
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

/// T9 评审补 (静默重置洞): 时钟偏移/时区是用户调过的现场 —— 加/减源重归并
/// 必须**按路径随行** (旧 carry_view_state 只搬显隐与键，偏移会被重置成默认)。
#[test]
fn rebuild_merge_keeps_time_params_by_path() {
    let (mut app, cur, a, b) = merge_fixture("t9-params");
    open_single(&mut app, &cur);
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone(), b.clone()]);
    {
        let tz = app.merge.as_ref().unwrap().sources[1].tz_offset_ms;
        app.merge.as_mut().unwrap().set_time_params(1, -3000, tz);
    }
    // 加一个源 (走 rebuild_merge 全量重跑)
    app.rebuild_merge(vec![cur.clone(), a.clone(), b.clone()]);
    pump_merge(&mut app);
    let m = app.merge.as_ref().unwrap();
    assert_eq!(m.sources.len(), 3);
    assert_eq!(
        m.sources[1].offset_ms, -3000,
        "时钟偏移按路径随行 (加/减源不重置)"
    );
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

/// spec §5 宣称的产品级锁 (家法核对补): 藏一个源 → 时间线按掩码重建
/// (行数减、余序不乱) → 恢复 → 全序回来。
#[test]
fn merge_source_hide_rebuilds_and_restores_order() {
    let (mut app, cur, a, b) = merge_fixture("t9-hide");
    app.entitlement = Entitlement::Paid {
        source: PaidSource::StoreAddOn,
    };
    open_single(&mut app, &cur);
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone(), b.clone()]);
    let full: Vec<(u32, u32)> = {
        let m = app.merge.as_ref().unwrap();
        (0..m.row_count())
            .map(|p| {
                let r = m.row_at(p).unwrap();
                (r.src, r.line)
            })
            .collect()
    };
    assert_eq!(full.len(), 9);
    // 藏源 2 (b.jsonl, 3 行)
    app.update(Msg::ToggleMergeSource(b.to_string_lossy().into_owned()));
    let hidden_rows: Vec<(u32, u32)> = {
        let m = app.merge.as_ref().unwrap();
        assert_eq!(m.row_count(), 6, "隐藏源行不进时间线");
        (0..m.row_count())
            .map(|p| {
                let r = m.row_at(p).unwrap();
                (r.src, r.line)
            })
            .collect()
    };
    let expect: Vec<(u32, u32)> = full.iter().copied().filter(|r| r.0 != 2).collect();
    assert_eq!(hidden_rows, expect, "余序不乱 (掩码不改其余源相对序)");
    // 恢复
    app.update(Msg::ToggleMergeSource(b.to_string_lossy().into_owned()));
    let back: Vec<(u32, u32)> = {
        let m = app.merge.as_ref().unwrap();
        assert_eq!(m.row_count(), 9, "恢复后全序回来");
        (0..m.row_count())
            .map(|p| {
                let r = m.row_at(p).unwrap();
                (r.src, r.line)
            })
            .collect()
    };
    assert_eq!(back, full, "恢复后序与原全序逐位相同");
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

/// T9 D4: 追加落在**选中行之前** → 位置整体推后，选中仍钉同一 (源，行)。
/// (有界窗口重定位; 窗口按 [原位 - 补全，原位 + 批行数] 取，编不出更远。)
#[test]
fn append_source_keeps_selection_on_same_row() {
    let (mut app, cur, a, b) = merge_fixture("t9-anchor");
    open_single(&mut app, &cur);
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone(), b.clone()]);
    // fixture 归并序：c0 a0 b0 a1 b1 b2 c1 c2 a2 —— 位 7 = (源 0, 行 2)
    app.merge.as_mut().unwrap().selected = 7;
    {
        use std::io::Write;
        let mut w = std::fs::OpenOptions::new().append(true).open(&a).unwrap();
        w.write_all(b"2026-09-27T00:00:02.500Z INFO a0b\n").unwrap();
    }
    app.poll_growth();
    let m = app.merge.as_ref().unwrap();
    assert_eq!(m.row_count(), 10, "新行进时间线");
    let at3 = m.row_at(3).unwrap();
    assert_eq!((at3.src, at3.line), (1, 3), "新行落在位 3 (选中之前)");
    let r = m.row_at(m.selected).unwrap();
    assert_eq!(
        (r.src, r.line),
        (0, 2),
        "选中仍钉同一 (源，行): {:?}",
        m.selected
    );
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}

/// T9 产品闸：合并源单轮增长超 [`MERGE_SYNC_MAX_ROWS`] → 交 worker 全量
/// 重归并 (旧 bundle 保持可见), 不在 UI 线程做增量合流。
#[test]
fn merge_large_growth_routes_to_rebuild_not_incremental() {
    let (mut app, cur, a, b) = merge_fixture("t9-gate");
    open_single(&mut app, &cur);
    apply_merge_sync(&mut app, vec![cur.clone(), a.clone(), b.clone()]);
    let before = app.merge.as_ref().unwrap().row_count();
    let n = MERGE_SYNC_MAX_ROWS + 1;
    {
        use std::io::Write;
        let mut w = std::fs::OpenOptions::new().append(true).open(&a).unwrap();
        let mut buf = String::new();
        for i in 0..n {
            buf.push_str(&format!(
                "2026-09-27T00:10:{:02}.000Z INFO bulk {i}\n",
                i % 60
            ));
        }
        w.write_all(buf.as_bytes()).unwrap();
    }
    app.poll_growth();
    assert!(
        app.merge_job_live,
        "超闸增量 → worker 重归并在途，不走同步合流"
    );
    assert_eq!(
        app.merge.as_ref().unwrap().row_count(),
        before,
        "旧 bundle 未动 (重归并在途期间保持可见)"
    );
    pump_merge(&mut app);
    assert_eq!(
        app.merge.as_ref().unwrap().row_count(),
        before + n,
        "重归并交卷后新行全进时间线"
    );
    std::fs::remove_file(&cur).ok();
    std::fs::remove_file(&a).ok();
    std::fs::remove_file(&b).ok();
}
