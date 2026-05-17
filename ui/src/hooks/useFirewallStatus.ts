import { useEffect, useState } from 'react';
import { getFirewallStatus } from '../ipc/commands';

export function useFirewallStatus() {
  const [isConfigured, setIsConfigured] = useState(true);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    const checkStatus = async () => {
      try {
        const configured = await getFirewallStatus();
        setIsConfigured(configured);
      } catch (e) {
        console.error('Failed to get firewall status:', e);
        setIsConfigured(true);
      } finally {
        setIsLoading(false);
      }
    };

    checkStatus();
  }, []);

  return { isConfigured, setIsConfigured, isLoading };
}
