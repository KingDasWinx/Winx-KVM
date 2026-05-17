import { Alert, Button, Group } from '@mantine/core';
import { useTranslation } from 'react-i18next';
import { useState } from 'react';
import { reconfigureFirewall, exportDiagnostics } from '../ipc/commands';

interface FirewallBannerProps {
  isConfigured: boolean;
  onConfigured?: () => void;
}

export function FirewallBanner({ isConfigured, onConfigured }: FirewallBannerProps) {
  const { t } = useTranslation('common');
  const [isLoading, setIsLoading] = useState(false);
  const [isExporting, setIsExporting] = useState(false);

  if (isConfigured) {
    return null;
  }

  const handleReconfigure = async () => {
    setIsLoading(true);
    try {
      await reconfigureFirewall();
      onConfigured?.();
    } catch (e) {
      console.error('Failed to reconfigure firewall:', e);
    } finally {
      setIsLoading(false);
    }
  };

  const handleExportDiagnostics = async () => {
    setIsExporting(true);
    try {
      const diagnostics = await exportDiagnostics();
      const blob = new Blob([diagnostics], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `winx-kvm-diagnostics-${new Date().toISOString()}.json`;
      a.click();
      URL.revokeObjectURL(url);
    } catch (e) {
      console.error('Failed to export diagnostics:', e);
    } finally {
      setIsExporting(false);
    }
  };

  return (
    <Alert
      icon={null}
      title={t('firewall.banner_title')}
      color="yellow"
      mb="md"
    >
      <Group justify="space-between" align="center">
        <span>{t('firewall.banner_desc')}</span>
        <Group gap="xs">
          <Button
            size="xs"
            variant="light"
            onClick={handleExportDiagnostics}
            loading={isExporting}
          >
            {isExporting ? t('common.exporting') : t('common.export_diagnostics')}
          </Button>
          <Button
            size="xs"
            onClick={handleReconfigure}
            loading={isLoading}
          >
            {isLoading ? t('firewall.reconfiguring') : t('firewall.reconfigure_button')}
          </Button>
        </Group>
      </Group>
    </Alert>
  );
}
