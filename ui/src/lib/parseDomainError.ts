import { notifications } from '@mantine/notifications';
import type { TFunction } from 'i18next';

/** Converte code serializado pelo backend (`transport_peer_not_trusted`) em chave i18n. */
export function domainCodeToI18nKey(code: string): string {
  const prefixes = [
    'identity_',
    'pairing_',
    'transport_',
    'input_',
    'media_',
    'clipboard_',
    'file_transfer_',
    'general_',
  ] as const;

  for (const prefix of prefixes) {
    if (code.startsWith(prefix)) {
      const context = prefix.slice(0, -1);
      const kind = code.slice(prefix.length);
      return `error.${context}.${kind}`;
    }
  }

  if (code.includes('.')) {
    return `error.${code}`;
  }

  return 'error.general.internal_error';
}

export function parseDomainError(err: unknown): { code: string } | null {
  if (typeof err !== 'string') {
    return null;
  }

  try {
    const parsed = JSON.parse(err) as { code?: string };
    if (typeof parsed.code !== 'string') {
      return null;
    }
    return { code: parsed.code };
  } catch {
    return null;
  }
}

export function notifyDomainError(err: unknown, t: TFunction<'common'>) {
  const parsed = parseDomainError(err);
  const key = parsed ? domainCodeToI18nKey(parsed.code) : 'error.general.internal_error';
  notifications.show({
    title: t('transport.connect_button'),
    message: t(key, { defaultValue: key }),
    color: 'red',
  });
}
