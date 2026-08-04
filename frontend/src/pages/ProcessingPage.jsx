import { useState, useEffect } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import Layout from '../components/Layout';
import api from '../lib/axios';

export default function ProcessingPage() {
  const { orderId } = useParams();
  const navigate = useNavigate();
  const [error, setError] = useState('');

  useEffect(() => {
    let cancelled = false;
    const poll = async () => {
      try {
        const { data } = await api.get(`/orders/${orderId}/status/`);
        if (cancelled) return;
        if (data.status === 'COMPLETED') {
          navigate(`/receipt/${orderId}`);
        } else if (data.status === 'FAILED') {
          setError(data.failure_reason || 'Order processing failed.');
        } else {
          setTimeout(poll, 2000);
        }
      } catch {
        if (!cancelled) setError('Failed to check order status.');
      }
    };
    poll();
    return () => { cancelled = true; };
  }, [orderId, navigate]);

  return (
    <Layout>
      <div className="flex flex-col items-center justify-center py-24">
        {error ? (
          <div className="text-center">
            <div className="bg-red-50 text-red-700 px-6 py-4 rounded-lg mb-4">
              {error}
            </div>
            <button
              onClick={() => navigate('/catalog')}
              className="text-indigo-600 hover:text-indigo-800"
            >
              Back to Catalog
            </button>
          </div>
        ) : (
          <>
            <div className="animate-spin rounded-full h-16 w-16 border-t-4 border-indigo-600 mb-6"></div>
            <h2 className="text-xl font-semibold text-gray-900">Processing Order</h2>
            <p className="text-gray-500 mt-2">
              Your order is being processed. This may take a moment...
            </p>
            <p className="text-sm text-gray-400 mt-4">
              Order ID: {orderId}
            </p>
          </>
        )}
      </div>
    </Layout>
  );
}
