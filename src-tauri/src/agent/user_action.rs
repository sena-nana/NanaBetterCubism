use crate::agent::store::PendingQuestion;
use crate::agent::PlanApprovalAction;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum PendingUserAction {
    Question {
        action_id: String,
        conversation_id: String,
        question: String,
        options: Vec<String>,
    },
    PlanApproval {
        #[serde(flatten)]
        approval: PlanApprovalAction,
    },
}

impl PendingUserAction {
    pub fn action_id(&self) -> &str {
        match self {
            Self::Question { action_id, .. } => action_id,
            Self::PlanApproval { approval } => &approval.action_id,
        }
    }
}

impl From<PlanApprovalAction> for PendingUserAction {
    fn from(approval: PlanApprovalAction) -> Self {
        Self::PlanApproval { approval }
    }
}

impl From<PendingQuestion> for PendingUserAction {
    fn from(value: PendingQuestion) -> Self {
        Self::Question {
            action_id: value.action_id,
            conversation_id: value.conversation_id,
            question: value.question,
            options: value.options,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_approval_uses_the_unified_frontend_contract() {
        let action = PendingUserAction::from(PlanApprovalAction {
            action_id: "plan".into(),
            conversation_id: "conversation".into(),
            title: "参数计划".into(),
        });
        let value = serde_json::to_value(action).unwrap();
        assert_eq!(value["kind"], "plan_approval");
        assert_eq!(value["actionId"], "plan");
        assert_eq!(value["conversationId"], "conversation");
    }
}
