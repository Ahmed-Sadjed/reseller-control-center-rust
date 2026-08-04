import { useState, useEffect, useCallback } from 'react';
import api from '../lib/axios';

export function useBalance() {
  const [balance, setBalance] = useState(null);

  const fetchBalance = useCallback(async () => {
    try {
      const { data } = await api.get('/auth/me/');
      setBalance(data.credit_balance);
    } catch {
    }
  }, []);

  useEffect(() => {
    fetchBalance();
    const interval = setInterval(fetchBalance, 30000);
    return () => clearInterval(interval);
  }, [fetchBalance]);

  return { balance, refresh: fetchBalance };
}
