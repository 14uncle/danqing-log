//! @author 十四叔
//! @date 2026/09/29
//! LogApp · 许可簇: key 激活、商店授权与购买流程。
use super::*;

impl LogApp {
    /// 激活付费层 (SPEC-v1x-licensing D4): 校验通过即**即时**翻转授权状态，
    /// 不要求重启; 落盘失败时状态照样翻转、只警示「重启后需重新激活」。
    /// 反馈落在「许可」页内 (`license_feedback`) —— 底栏 notice 会被模态卡遮住。
    pub(crate) fn activate_license(&mut self, key: String) {
        match license::activate(&key, &self.license_path(), &self.license_pubkey) {
            Ok(payload) => {
                let tier_text = match payload.tier {
                    license::Tier::Personal => "个人版",
                    license::Tier::Enterprise => "企业版",
                };
                self.entitlement = Entitlement::Paid {
                    source: PaidSource::License(payload),
                };
                // 激活成功 = key 已落盘，输入框里的明文清掉 (安全评审：key 是
                // 用户资产，不该留在卡面上; rev 驱动 widget 侧清空，见
                // 设置卡 key_input_box 的 bind_clear)。Persist 分支保留原文
                // (用户可能要重试复制)。
                self.license_key_input.clear();
                self.license_clear_rev += 1;
                self.license_feedback =
                    Some((format!("已激活付费层（{tier_text}）"), NoticeKind::Info));
            }
            Err(license::ActivateError::Persist(detail)) => {
                // key 是真的但没落盘 —— 照样激活本次会话 (D4 即时生效),
                // 重新验一次拿载荷 (verify_key 是纯函数，成本可忽略)。
                if let Ok(payload) = license::verify_key(&key, &self.license_pubkey) {
                    self.entitlement = Entitlement::Paid {
                        source: PaidSource::License(payload),
                    };
                }
                self.license_feedback = Some((
                    format!("已激活，但保存失败（重启后需重新激活）: {detail}"),
                    NoticeKind::Warn,
                ));
            }
            Err(license::ActivateError::Key(e)) => {
                let text = match e {
                    license::KeyError::Malformed => {
                        "key 格式不对 —— 请完整复制邮件里的整串 key".to_string()
                    }
                    license::KeyError::BadSignature => {
                        "key 校验失败 —— 内容可能被篡改，或不是本应用签发的 key".to_string()
                    }
                    license::KeyError::WrongProduct => {
                        "这是其他产品的 key，不能用于丹青日志".to_string()
                    }
                };
                self.license_feedback = Some((text, NoticeKind::Warn));
            }
        }
    }

    /// 商店授权查询回来了 (T5)。D4: **只许 Free → 其他** —— 会话内已激活的
    /// 授权 (贴 key) 不被晚到的商店查询覆盖 (会话内不踢人)。
    pub(crate) fn adopt_store_entitlement(&mut self, e: Entitlement) {
        if self.entitlement == Entitlement::Free {
            self.entitlement = e;
        }
    }

    /// 购买结果回来了 (T5)。成功即永久解锁 (买断); 取消/失败只反馈。
    /// 反馈落 `license_feedback` 而非底栏 notice —— 购买从设置卡/升级提示
    /// 发起，卡开着时底栏被模态遮住 (评审 Required; 与激活反馈同通道)。
    pub(crate) fn adopt_purchase_outcome(&mut self, outcome: store_license::PurchaseOutcome) {
        self.purchase_in_flight = false;
        match outcome {
            store_license::PurchaseOutcome::Purchased => {
                self.entitlement = Entitlement::Paid {
                    source: PaidSource::StoreAddOn,
                };
                self.license_feedback =
                    Some(("已解锁付费层，感谢支持".to_string(), NoticeKind::Info));
            }
            store_license::PurchaseOutcome::Cancelled => {
                self.license_feedback = Some(("购买已取消".to_string(), NoticeKind::Info));
            }
            store_license::PurchaseOutcome::Failed => {
                // add-on 未进目录时商店会报「购买未完成」—— 上架窗口期属预期
                // (pomodoro 实测), add-on 进目录即自愈。
                self.license_feedback =
                    Some(("购买未完成 · 可稍后重试".to_string(), NoticeKind::Warn));
            }
        }
    }

    /// 「获取付费层」(T5 机制层; UI 点位在 T6)。商店版拉起购买对话框
    /// (后台线程，结果走 tick 拾取); 便携版开购买页 —— `PURCHASE_URL`
    /// 未回填时给提示，不打开死链接 (D8; 该分支在两个入口都被
    /// `show_purchase_button` 提前隐藏时实际不可达，留作防御)。
    pub(crate) fn purchase_paid_layer(&mut self) {
        // 已是付费层：不进购买流程 (评审 Optional —— 商店版已购再点会拿到
        // AlreadyPurchased 然后弹「感谢支持」, 措辞错位)。
        if matches!(self.entitlement, Entitlement::Paid { .. }) {
            self.license_feedback =
                Some(("你已是付费层，无需重复购买".to_string(), NoticeKind::Info));
            return;
        }
        if danqing::platform::is_packaged() {
            // 防重入 (评审 Required; pomodoro 的 PURCHASE_STATE CAS 在此处等价):
            // 在途时忽略再次发起 —— 重复 launch 会拉多个系统购买框，且晚到的
            // 旧代次结果覆盖新代次后被 poll 丢弃 = 付了钱会话内无感知。
            if !self.try_begin_purchase() {
                self.license_feedback = Some(("购买正在进行中…".to_string(), NoticeKind::Info));
                return;
            }
            self.purchase_job
                .launch(store_license::purchase_full_version);
        } else {
            match license::PURCHASE_URL {
                Some(url) => {
                    if let Err(err) = open::that(url) {
                        log::warn!("打开购买页失败：{err}");
                    }
                }
                None => self.set_notice(
                    "购买页即将上线；已有 key 请直接在设置卡「许可」页激活".to_string(),
                    NoticeKind::Warn,
                ),
            }
        }
    }

    /// 购买发起闸 (纯状态，可测): 在途 → false; 空闲 → 置位 + true。
    /// 结果回来 (`adopt_purchase_outcome`) 复位。
    pub(crate) fn try_begin_purchase(&mut self) -> bool {
        if self.purchase_in_flight {
            false
        } else {
            self.purchase_in_flight = true;
            true
        }
    }
}
