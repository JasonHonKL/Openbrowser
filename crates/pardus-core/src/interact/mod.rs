pub mod actions;
pub mod action_plan;
pub mod auto_fill;
pub mod element;
pub mod form;
pub mod recording;
pub mod scroll;
pub mod wait;
#[cfg(feature = "js")]
pub mod js_interact;

pub use element::ElementHandle;
pub use actions::InteractionResult;
pub use form::FormState;
pub use scroll::ScrollDirection;
pub use action_plan::{ActionPlan, ActionType, PageType, SuggestedAction};
pub use auto_fill::{AutoFillValues, AutoFillResult, ValidationStatus};
pub use recording::{SessionRecording, SessionRecorder, RecordedAction, RecordedActionType, ReplayStepResult, replay};
pub use wait::{wait_for_selector, WaitCondition, wait_smart};
#[cfg(feature = "js")]
pub use js_interact::{js_click, js_type, js_scroll, js_submit};
