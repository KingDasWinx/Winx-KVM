import { NavLink, Stack, Text, Title } from '@mantine/core';
import { IconHome, IconSettings } from '@tabler/icons-react';
import { useTranslation } from 'react-i18next';
import { useLocation, useNavigate } from 'react-router-dom';

export function AppSidebar() {
  const { t } = useTranslation('common');
  const navigate = useNavigate();
  const { pathname } = useLocation();

  return (
    <Stack gap="md" p="md" h="100%">
      <Title order={4}>{t('app.name')}</Title>
      <Text size="xs" c="dimmed">
        {t('app.tagline')}
      </Text>

      <Stack gap={4} mt="md">
        <NavLink
          label={t('nav.home')}
          leftSection={<IconHome size={18} stroke={1.5} />}
          active={pathname === '/'}
          onClick={() => navigate('/')}
        />
        <NavLink
          label={t('nav.settings')}
          leftSection={<IconSettings size={18} stroke={1.5} />}
          active={pathname === '/settings'}
          onClick={() => navigate('/settings')}
        />
      </Stack>
    </Stack>
  );
}
