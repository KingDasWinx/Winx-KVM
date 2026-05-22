import { useEffect, useState } from 'react';
import { Button, Checkbox, Group, Modal, Stack, Text, TextInput } from '@mantine/core';
import { useTranslation } from 'react-i18next';
import { useWorkspaceStore } from '../../store/workspaceStore';
import * as ipc from '../../ipc/commands';

interface Props {
  opened: boolean;
  onClose: () => void;
  initialPeerIds?: string[];
}

export default function CreateWorkspaceModal({ opened, onClose, initialPeerIds }: Props) {
  const { t } = useTranslation('workspace');
  const { refresh } = useWorkspaceStore();
  const [name, setName] = useState('');
  const [peers, setPeers] = useState<ipc.DiscoveredPeer[]>([]);
  const [selectedPeerIds, setSelectedPeerIds] = useState<Set<string>>(
    new Set(initialPeerIds || [])
  );
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (opened) {
      loadPeers();
      if (initialPeerIds) {
        setSelectedPeerIds(new Set(initialPeerIds));
      }
    }
  }, [opened, initialPeerIds]);

  const loadPeers = async () => {
    try {
      const discoveredPeers = await ipc.listDiscoveredPeers();
      setPeers(discoveredPeers);
    } catch (err) {
      console.error('Failed to load peers:', err);
    }
  };

  const handleCreate = async () => {
    setError(null);
    if (!name.trim()) {
      setError(t('createModal.errors.nameEmpty'));
      return;
    }
    if (selectedPeerIds.size === 0) {
      setError(t('createModal.errors.noTargetSelected'));
      return;
    }

    setLoading(true);
    try {
      await ipc.createWorkspace(name, Array.from(selectedPeerIds));
      await refresh();
      setName('');
      setSelectedPeerIds(new Set());
      onClose();
    } catch (err) {
      console.error('Failed to create workspace:', err);
      setError(typeof err === 'string' ? err : 'Failed to create workspace');
    } finally {
      setLoading(false);
    }
  };

  const handleTogglePeer = (peerId: string) => {
    const newSelection = new Set(selectedPeerIds);
    if (newSelection.has(peerId)) {
      newSelection.delete(peerId);
    } else {
      newSelection.add(peerId);
    }
    setSelectedPeerIds(newSelection);
  };

  return (
    <Modal opened={opened} onClose={onClose} title={t('createModal.title')} centered>
      <Stack gap="md">
        <TextInput
          label={t('createModal.nameLabel')}
          placeholder={t('createModal.namePlaceholder')}
          value={name}
          onChange={(e) => setName(e.currentTarget.value)}
          disabled={loading}
        />

        <div>
          <Text size="sm" fw={600} mb="xs">
            {t('createModal.peersLabel')}
          </Text>
          <Text size="xs" c="dimmed" mb="sm">
            {t('createModal.peersHint')}
          </Text>
          {peers.length === 0 ? (
            <Text size="sm" c="dimmed">
              {t('createModal.noPeersDiscovered')}
            </Text>
          ) : (
            <Stack gap="xs">
              {peers.map((peer) => (
                <Checkbox
                  key={peer.id}
                  label={peer.username}
                  checked={selectedPeerIds.has(peer.id)}
                  onChange={() => handleTogglePeer(peer.id)}
                  disabled={loading}
                />
              ))}
            </Stack>
          )}
        </div>

        {error && (
          <Text size="sm" c="red">
            {error}
          </Text>
        )}

        <Group justify="flex-end" gap="xs">
          <Button variant="subtle" onClick={onClose} disabled={loading}>
            {t('createModal.cancelButton')}
          </Button>
          <Button variant="filled" onClick={handleCreate} loading={loading}>
            {t('createModal.createButton')}
          </Button>
        </Group>
      </Stack>
    </Modal>
  );
}
