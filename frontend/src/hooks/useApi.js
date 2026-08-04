import { useState, useCallback } from 'react';
import api from '../lib/axios';

export function useApi(url, options = {}) {
  const [data, setData] = useState(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);

  const execute = useCallback(async (config = {}) => {
    setLoading(true);
    setError(null);
    try {
      const { data: result } = await api({ url, ...options, ...config });
      setData(result);
      return result;
    } catch (err) {
      const msg = err.response?.data?.error || err.message || 'An error occurred';
      setError(msg);
      throw err;
    } finally {
      setLoading(false);
    }
  }, [url]);

  return { data, loading, error, execute };
}
