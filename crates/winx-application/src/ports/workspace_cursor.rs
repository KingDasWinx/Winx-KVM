use async_trait::async_trait;

/// Port para publicar e restaurar o cursor global do workspace ativo.
#[async_trait]
pub trait WorkspaceGlobalCursor: Send + Sync {
    /// Publica a posição local do cursor no workspace conectado.
    async fn publish_local_cursor(&self, x: i32, y: i32);

    /// Retorna a última posição conhecida do cursor global ao assumir foco local.
    async fn restore_cursor_on_focus(&self) -> Option<(i32, i32)>;
}
