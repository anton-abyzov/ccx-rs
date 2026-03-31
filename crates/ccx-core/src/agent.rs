use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot, Mutex};

/// Definition of an agent to spawn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDef {
    pub name: String,
    pub description: String,
    pub prompt: String,
    #[serde(default)]
    pub background: bool,
}

/// Result from a completed agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResult {
    pub name: String,
    pub output: String,
    pub success: bool,
}

/// Message that can be sent to a named agent.
#[derive(Debug)]
pub struct AgentMessage {
    pub from: String,
    pub content: String,
    pub reply_tx: Option<oneshot::Sender<String>>,
}

/// Manages spawned subagents.
pub struct AgentManager {
    agents: Arc<Mutex<HashMap<String, AgentHandle>>>,
}

struct AgentHandle {
    tx: mpsc::Sender<AgentMessage>,
    join: tokio::task::JoinHandle<AgentResult>,
}

impl AgentManager {
    pub fn new() -> Self {
        Self {
            agents: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Spawn a new agent that runs the given async function.
    pub async fn spawn<F, Fut>(&self, def: AgentDef, work: F) -> mpsc::Receiver<AgentMessage>
    where
        F: FnOnce(AgentDef, mpsc::Receiver<AgentMessage>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = AgentResult> + Send,
    {
        let (_tx, rx) = mpsc::channel(32);
        let (work_tx, work_rx) = mpsc::channel(32);
        let name = def.name.clone();

        let join = tokio::spawn(async move {
            work(def, work_rx).await
        });

        let handle = AgentHandle {
            tx: work_tx,
            join,
        };

        self.agents.lock().await.insert(name, handle);
        rx
    }

    /// Send a message to a named agent.
    pub async fn send(&self, agent_name: &str, msg: AgentMessage) -> Result<(), AgentError> {
        let agents = self.agents.lock().await;
        let handle = agents
            .get(agent_name)
            .ok_or_else(|| AgentError::NotFound(agent_name.into()))?;
        handle
            .tx
            .send(msg)
            .await
            .map_err(|_| AgentError::ChannelClosed(agent_name.into()))?;
        Ok(())
    }

    /// Wait for an agent to complete and return its result.
    pub async fn wait(&self, agent_name: &str) -> Result<AgentResult, AgentError> {
        let handle = self
            .agents
            .lock()
            .await
            .remove(agent_name)
            .ok_or_else(|| AgentError::NotFound(agent_name.into()))?;
        handle
            .join
            .await
            .map_err(|e| AgentError::JoinError(e.to_string()))
    }

    /// List names of active agents.
    pub async fn active_agents(&self) -> Vec<String> {
        self.agents
            .lock()
            .await
            .keys()
            .cloned()
            .collect()
    }
}

impl Default for AgentManager {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("agent not found: {0}")]
    NotFound(String),
    #[error("channel closed for agent: {0}")]
    ChannelClosed(String),
    #[error("join error: {0}")]
    JoinError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_agent_manager_spawn_and_wait() {
        let manager = AgentManager::new();
        let def = AgentDef {
            name: "test-agent".into(),
            description: "Test".into(),
            prompt: "Do something".into(),
            background: false,
        };

        let _rx = manager
            .spawn(def, |def, _rx| async move {
                AgentResult {
                    name: def.name,
                    output: "done".into(),
                    success: true,
                }
            })
            .await;

        let result = manager.wait("test-agent").await.unwrap();
        assert_eq!(result.name, "test-agent");
        assert_eq!(result.output, "done");
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_agent_manager_active_agents() {
        let manager = AgentManager::new();
        let def = AgentDef {
            name: "long-agent".into(),
            description: "Long running".into(),
            prompt: "Work".into(),
            background: true,
        };

        let _rx = manager
            .spawn(def, |_def, mut rx| async move {
                // Wait for a message or just return.
                let _ = rx.recv().await;
                AgentResult {
                    name: "long-agent".into(),
                    output: "done".into(),
                    success: true,
                }
            })
            .await;

        let active = manager.active_agents().await;
        assert!(active.contains(&"long-agent".to_string()));
    }

    #[tokio::test]
    async fn test_agent_not_found() {
        let manager = AgentManager::new();
        let err = manager.wait("nonexistent").await.unwrap_err();
        assert!(matches!(err, AgentError::NotFound(_)));
    }
}
