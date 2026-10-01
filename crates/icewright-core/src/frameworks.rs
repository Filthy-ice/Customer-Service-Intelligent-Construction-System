//! 生成物 Agent 框架选型知识库。
//!
//! 原则：生成的客服系统绝不手搓驱动 AI 的框架，必须站在成熟 agent 框架上集成。
//! 本表是 2026-10-01 实时调研快照（GitHub star/推送活跃度 + 当月横评文章）：
//! - 设计文档（S4）渲染选型与候选对比，随闸门A 由人确认；
//! - `workspace.agent_framework` 可覆盖每栈默认选择（值须命中该栈候选名）；
//! - `Integration::Planned` 表示 S5 模板尚未落地该框架集成，闸门A 须知会如实标注。

use anyhow::{bail, Result};

/// S5 模板对该框架的集成状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Integration {
    /// 模板已按该框架构建生成物
    Implemented,
    /// 选型已定，模板集成待实现（设计文档如实标注，绝不冒充交付）
    Planned,
}

#[derive(Debug, Clone, Copy)]
pub struct Framework {
    /// 配置覆盖时使用的规范名（小写连字符）
    pub name: &'static str,
    /// 调研快照时的版本线
    pub version_line: &'static str,
    /// 调研快照时的 GitHub star 数（约数）
    pub stars: &'static str,
    pub pros: &'static str,
    pub cons: &'static str,
    pub integration: Integration,
}

/// python 栈候选（2026-10-01 快照）
const PYTHON: &[Framework] = &[
    Framework {
        name: "agentscope",
        version_line: "2.0.x",
        stars: "32.6k",
        pros: "skill/工具驱动的 ReAct 与多智能体，OpenAI-compatible 模型接入，社区活跃日更",
        cons: "复杂状态图能力弱于 LangGraph（需以规则引擎侧兜底，恰合本系统确定性规则定位）",
        integration: Integration::Implemented,
    },
    Framework {
        name: "langgraph",
        version_line: "1.x",
        stars: "42.6k",
        pros: "显式状态图：节点/条件边/检查点全程可调试，生产编排最强",
        cons: "抽象面广学习曲线陡，历史上 API 迁移频繁",
        integration: Integration::Planned,
    },
    Framework {
        name: "pydantic-ai",
        version_line: "1.x",
        stars: "20.3k",
        pros: "类型化输出可靠，代码量小适合轻量单 agent",
        cons: "多智能体编排与技能装载能力有限",
        integration: Integration::Planned,
    },
];

/// java 栈候选（2026-10-01 快照）
const JAVA: &[Framework] = &[
    Framework {
        name: "spring-ai",
        version_line: "1.1.x 稳定线 / 2.0 GA",
        stars: "9.5k",
        pros: "Spring 官方，ToolCallingAdvisor 统一工具执行，MCP 原生，适配 JDK 21 虚拟线程",
        cons: "2.x 迭代快，部分 starter 仍处里程碑版",
        integration: Integration::Planned,
    },
    Framework {
        name: "langchain4j",
        version_line: "1.x",
        stars: "3k+",
        pros: "生态广、模型供应商适配多",
        cons: "非 Spring 原生的 advisor/自动配置心智，装配代码偏多",
        integration: Integration::Planned,
    },
];

/// go 栈候选（2026-10-01 快照）
const GO: &[Framework] = &[
    Framework {
        name: "eino",
        version_line: "0.9.x（CloudWeGo）",
        stars: "13.2k",
        pros: "组件化编排工程化深，字节维护活跃，eino-ext 提供 OpenAI-compatible 协议组件",
        cons: "未及 1.0，API 仍可能变动",
        integration: Integration::Planned,
    },
    Framework {
        name: "genkit-go",
        version_line: "1.x",
        stars: "3k+",
        pros: "类型安全流式原语，Firebase 生态背书",
        cons: "agent 编排生态薄，国内模型适配资料少",
        integration: Integration::Planned,
    },
    Framework {
        name: "langchaingo",
        version_line: "0.1.x",
        stars: "9.7k",
        pros: "LangChain 心智平移",
        cons: "已约 9 个月无实质更新（快照时最后推送 2026-01），不推荐",
        integration: Integration::Planned,
    },
];

/// 某栈的候选列表；未知栈返回空（S2/S5 已先行拒绝非法栈）。
pub fn candidates(stack: &str) -> &'static [Framework] {
    match stack {
        "python" => PYTHON,
        "java" => JAVA,
        "go" => GO,
        _ => &[],
    }
}

/// 每栈默认 = 候选首位（调研评估结论）。
pub fn default_for(stack: &str) -> Option<&'static Framework> {
    candidates(stack).first()
}

/// 解析选型：`workspace.agent_framework` 非空则必须命中该栈候选名（大小写不敏感），
/// 否则用栈默认。空栈或未收录栈返回 None（设计文档跳过选型章节）。
pub fn resolve(stack: &str, override_name: &str) -> Result<Option<&'static Framework>> {
    let override_name = override_name.trim();
    if override_name.is_empty() {
        return Ok(default_for(stack));
    }
    let hit = candidates(stack)
        .iter()
        .find(|f| f.name.eq_ignore_ascii_case(override_name));
    match hit {
        Some(f) => Ok(Some(f)),
        None => {
            let names: Vec<&str> = candidates(stack).iter().map(|f| f.name).collect();
            bail!(
                "workspace.agent_framework={override_name:?} 不在 {stack} 栈候选中（可选：{}）",
                names.join(", ")
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_first_candidate_per_stack() {
        for stack in ["python", "java", "go"] {
            assert_eq!(
                default_for(stack).map(|f| f.name),
                Some(candidates(stack)[0].name),
                "{stack} 默认应命中候选首位"
            );
        }
        assert!(default_for("rust").is_none());
    }

    #[test]
    fn override_must_match_candidate_names() {
        let f = resolve("java", "Spring-AI").unwrap();
        assert_eq!(f.map(|x| x.name), Some("spring-ai"));
        assert_eq!(
            resolve("python", "").unwrap().map(|f| f.name),
            Some("agentscope")
        );
        assert!(resolve("python", "tensorflow").is_err());
        assert!(resolve("go", "spring-ai").is_err());
    }
}
