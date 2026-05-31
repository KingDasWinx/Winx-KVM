import { useEffect, useState } from 'react';
import { Button, Checkbox, Modal, Stack, Text } from '@mantine/core';
import { useTranslation } from 'react-i18next';
import * as ipc from '../../ipc/commands';
import type { DiscoveredPeer } from '../../ipc/commands';

interface Props {
  workspaceId: string | null;
  onClose: () => void;
}

export default function InvitePeerModal({ workspaceId, onClose }: Props) {
  const { t } = useTranslation('workspace');
  const [peers, setPeers] = useState<DiscoveredPeer[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());

  useEffect(() => {
    if (workspaceId) {
      ipc.listDiscoveredPeers().then(setPeers).catch(console.error);
    }
  }, [workspaceId]);

  const handleInvite = async () => {
    if (!workspaceId) return;
    for (const peerId of selected) {
      await ipc.inviteToWorkspace(workspaceId, peerId).catch(console.error);
    }
    onClose();
  };

  return (
    <Modal opened={!!workspaceId} onClose={onClose} title={t('invitePeer.title')}>
      <Stack gap="xs">
        {peers.length === 0 ? (
          <Text c="dimmed">{t('invitePeer.noPeers')}</Text>
        ) : (
          peers.map((p) => (
            <Checkbox
              key={p.id}
              label={p.username}
              checked={selected.has(p.id)}
              onChange={(e) => {
                const next = new Set(selected);
                if (e.currentTarget.checked) next.add(p.id);
                else next.delete(p.id);
                setSelected(next);
              }}
            />
          ))
        )}
        <Button onClick={handleInvite} disabled={selected.size === 0}>
          {t('invitePeer.inviteButton', { count: selected.size })}
        </Button>
      </Stack>
    </Modal>
  );
}
