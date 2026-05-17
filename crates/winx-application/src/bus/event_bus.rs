//! Event bus baseado em `tokio::sync::broadcast`.
//!
//! Cada bounded context publica `DomainEvent`s; outros contexts (e a UI via
//! Tauri) assinam o bus e reagem ao que importa. Capacidade do canal é
//! generosa porque eventos de input podem disparar em rajada.

use tokio::sync::broadcast::{self, error::SendError, Receiver, Sender};
use winx_domain::DomainEvent;

/// Capacidade do canal. Eventos descartados se subscribers lentos.
const DEFAULT_CAPACITY: usize = 1024;

/// Wrapper em torno de `tokio::sync::broadcast::Sender<DomainEvent>`.
///
/// Clones compartilham o mesmo canal. Use `subscribe()` para obter um
/// `Receiver` que recebe eventos publicados a partir daquele momento.
#[derive(Debug, Clone)]
pub struct EventBus {
    tx: Sender<DomainEvent>,
}

impl EventBus {
    /// Cria um event bus com capacidade padrão.
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    /// Cria um event bus com capacidade customizada.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Publica um evento. Não falha quando não há subscribers — apenas
    /// registra um trace.
    pub fn publish(&self, event: DomainEvent) {
        if let Err(SendError(dropped)) = self.tx.send(event) {
            // Sem subscribers: comportamento esperado quando o app inicia
            // antes da UI conectar. Logamos em debug.
            tracing::debug!(?dropped, "evento publicado sem subscribers");
        }
    }

    /// Subscribe para receber novos eventos a partir deste momento.
    #[must_use]
    pub fn subscribe(&self) -> Receiver<DomainEvent> {
        self.tx.subscribe()
    }

    /// Número atual de subscribers ativos.
    #[must_use]
    pub fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use winx_domain::{identity::events::DeviceCreated, shared::ids::DeviceId};

    use super::*;

    fn sample_event() -> DomainEvent {
        DomainEvent::DeviceCreated(DeviceCreated {
            device_id: DeviceId::new(),
            fingerprint: "AB:CD:EF:01:23:45:67:89".to_string(),
        })
    }

    #[tokio::test]
    async fn publish_with_no_subscribers_does_not_panic() {
        let bus = EventBus::new();
        bus.publish(sample_event());
        assert_eq!(bus.subscriber_count(), 0);
    }

    #[tokio::test]
    async fn subscriber_receives_event() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();
        bus.publish(sample_event());
        let evt = rx.recv().await.unwrap();
        assert!(matches!(evt, DomainEvent::DeviceCreated(_)));
    }
}
