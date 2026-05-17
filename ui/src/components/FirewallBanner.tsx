import { Alert, Button, Group } from '@mantine/core';
import { useTranslation } from 'react-i18next';
import { useState } from 'react';
import { reconfigureFirewall } from '../ipc/commands';

interface FirewallBannerProps {
  isConfigured: boolean;
  onConfigured?: () => void;
}

export function FirewallBanner({ isConfigured, onConfigured }: FirewallBannerProps) {
  const { t } = useTranslation('common');
  const [isLoading, setIsLoading] = useState(false);

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

  return (
    <Alert
      icon={null}
      title={t('firewall.banner_title')}
      color="yellow"
      mb="md"
    >
      <Group justify="space-between" align="center">
        <span>{t('firewall.banner_desc')}</span>
        <Button
          size="xs"
          onClick={handleReconfigure}
          loading={isLoading}
        >
          {isLoading ? t('firewall.reconfiguring') : t('firewall.reconfigure_button')}
        </Button>
      </Group>
    </Alert>
  );
}
