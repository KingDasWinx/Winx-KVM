# Connect Real + Feedback de Pareamento na UI

Cópia do plano executado em 2026-05-17. Ver implementação em:

- `crates/winx-application/src/use_cases/discovery.rs` — `list_peers_enriched`
- `crates/winx-kvm/src/commands/discovery.rs` — `DiscoveredPeerDto.is_paired`
- `ui/src/components/PeersPanel.tsx` — badges, botões condicionais, refresh em `pairing-completed`
- `ui/src/lib/parseDomainError.ts` — erros de `open_connection` visíveis
