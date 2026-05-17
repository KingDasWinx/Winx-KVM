import { AppShell, Container } from '@mantine/core';
import { Outlet } from 'react-router-dom';

import { AppSidebar } from '../components/AppSidebar';
import { FirewallBanner } from '../components/FirewallBanner';
import { useWinxToasts } from '../hooks/useWinxToasts';
import { useFirewallStatus } from '../hooks/useFirewallStatus';

export function AppLayout() {
  useWinxToasts();
  const { isConfigured, setIsConfigured } = useFirewallStatus();

  return (
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
  );
}
