# Pairing Real entre Dispositivos — implementado

Plano executado em 2026-05-17. Resumo do que foi entregue:

- **Protocolo:** `crates/winx-protocol/src/pairing.rs` — UDP 7879, mensagens Request/Response/Confirm/Cancel, `pin_commitment` SHA-256.
- **Port:** `PairingTransport` + `UdpPairingTransport` em infra.
- **Domínio:** `PairingSession::responder`, `PairingIncoming`, papéis Initiator/Responder.
- **Aplicação:** `PairingService` envia/recebe datagramas, completa trust nos dois lados, QUIC após `PairingCompleted`.
- **UI:** evento `pairing-incoming` (toast só no responder); modal com PIN no initiator.
- **mDNS:** TXT `pubkey`; firewall regra UDP 7879.

Teste manual: ver [docs/TESTE-ENTRE-PCS.md](../TESTE-ENTRE-PCS.md).
