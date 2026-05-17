import { useEffect } from 'react';
import { AppShell, Container } from '@mantine/core';
import { Outlet } from 'react-router-dom';
import { useTranslation } from 'react-i18next';

import { AppSidebar } from '../components/AppSidebar';
import { FirewallBanner } from '../components/FirewallBanner';
import { IncomingPairingToast } from '../components/IncomingPairingToast';
import { useWinxToasts } from '../hooks/useWinxToasts';
import { useFirewallStatus } from '../hooks/useFirewallStatus';
import { enableClipboardSync, enableInputControl } from '../ipc/commands';
import { onWinxEvent } from '../ipc/events';
import { notifyDomainError } from '../lib/parseDomainError';

export function AppLayout() {
  useWinxToasts();
  const { t } = useTranslation('common');
  const { isConfigured, setIsConfigured } = useFirewallStatus();

  useEffect(() => {
    const unlisten = onWinxEvent((event) => {
      if (event.kind === 'connection-established' && event.peer_id) {
        enableInputControl(event.peer_id).catch((err: unknown) => {
          console.error('enable_input_control failed', err);
          notifyDomainError(err, t);
        });
        enableClipboardSync(event.peer_id).catch((err: unknown) => {
          console.error('enable_clipboard_sync failed', err);
          notifyDomainError(err, t);
        });
      }
    });

    return () => {
      unlisten.then((u) => u());
    };
  }, [t]);

  return (
    <>
      <AppShell
        navbar={{ width: 220, breakpoint: 'sm' }}
        padding="md"
      >
        <AppShell.Navbar p={0}>
          <AppSidebar />
        </AppShell.Navbar>
        <AppShell.Main>
          <Container size="md">
            <FirewallBanner
              isConfigured={isConfigured}
              onConfigured={() => setIsConfigured(true)}
            />
            <Outlet />
          </Container>
        </AppShell.Main>
      </AppShell>
      <IncomingPairingToast />
    </>
  );
}
