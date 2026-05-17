import { describe, expect, it } from 'vitest';

import { domainCodeToI18nKey, parseDomainError } from './parseDomainError';

describe('parseDomainError', () => {
  it('parses JSON domain error code', () => {
    const err = JSON.stringify({
      code: 'transport_peer_not_trusted',
      message: 'peer not in peers.toml',
    });
    expect(parseDomainError(err)).toEqual({ code: 'transport_peer_not_trusted' });
  });

  it('maps transport peer_not_trusted to i18n key', () => {
    expect(domainCodeToI18nKey('transport_peer_not_trusted')).toBe(
      'error.transport.peer_not_trusted',
    );
  });
});
