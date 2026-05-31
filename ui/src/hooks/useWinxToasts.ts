import { useEffect } from 'react';
import { notifications } from '@mantine/notifications';
import type { TFunction } from 'i18next';
import { useTranslation } from 'react-i18next';

import { onWinxEvent, type WinxEvent } from '../ipc/events';

function toastForEvent(event: WinxEvent, t: TFunction<'common'>) {
  switch (event.kind) {
    case 'pairing-completed':
      notifications.show({
        title: t('toast.pairing_completed_title'),
        message: t('toast.pairing_completed_message', {
          username: event.peer_username,
        }),
        color: 'green',
      });
      break;
    case 'pairing-failed':
      notifications.show({
        title: t('toast.pairing_failed_title'),
        message: t('toast.pairing_failed_message'),
        color: 'red',
      });
      break;
    case 'connection-established':
      if (event.is_inbound || event.workspace_id) {
        break;
      }
      notifications.show({
        title: t('toast.connection_established_title'),
        message: t('toast.connection_established_message', {
          username: event.peer_username,
        }),
        color: 'green',
      });
      break;
    case 'connection-lost':
      notifications.show({
        title: t('toast.connection_lost_title'),
        message: event.reason ?? t('toast.connection_lost_message'),
        color: 'yellow',
      });
      break;
    case 'clipboard-received':
      notifications.show({
        title: t('toast.clipboard_received_title'),
        message: t('toast.clipboard_received_message'),
        color: 'blue',
        autoClose: 3000,
      });
      break;
    default:
      break;
  }
}

export function useWinxToasts() {
  const { t } = useTranslation('common');

  useEffect(() => {
    const unlisten = onWinxEvent((event) => {
      toastForEvent(event, t);
    });

    return () => {
      unlisten.then((u) => u());
    };
  }, [t]);
}
