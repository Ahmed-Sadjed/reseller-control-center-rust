import { useState, useEffect } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import Layout from '../components/Layout';
import CredentialCard from '../components/CredentialCard';
import CredentialCardSkeleton from '../components/skeletons/CredentialCardSkeleton';
import api from '../lib/axios';
import { formatCredentialBlock, downloadTextFile } from '../lib/helpers';

export default function ReceiptPage() {
  const { orderId } = useParams();
  const navigate = useNavigate();
  const [credentials, setCredentials] = useState([]);
  const [order, setOrder] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');

  useEffect(() => {
    const fetchData = async () => {
      try {
        const [orderRes, credRes] = await Promise.all([
          api.get(`/orders/${orderId}/`),
          api.get(`/orders/${orderId}/credentials/`),
        ]);
        setOrder(orderRes.data);
        setCredentials(credRes.data);

        // Auto-open WhatsApp for WhatsApp product credentials
        const whatsappCred = credRes.data.find(c => c.credential_data?.wa_link);
        if (whatsappCred) {
          window.open(whatsappCred.credential_data.wa_link, '_blank');
        }
      } catch {
        setError('Failed to load receipt data.');
      } finally {
        setLoading(false);
      }
    };
    fetchData();
  }, [orderId]);

  const handleDownloadAll = () => {
    const allText = credentials
      .map((c, i) => `#${i + 1}\n` + formatCredentialBlock(c.username, c.password, c.dns_domain, c.m3u_url))
      .join('\n\n');
    downloadTextFile(allText, `receipt-${orderId}.txt`);
  };

  if (loading) {
    return (
      <Layout>
        <div className="space-y-6">
<div className="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4">
            <div>
              <div className="h-7 w-24 bg-gray-200 animate-pulse rounded" />
              <div className="h-4 w-48 bg-gray-200 animate-pulse rounded mt-1" />
            </div>
            <div className="h-9 w-32 bg-gray-200 animate-pulse rounded" />
          </div>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {Array.from({ length: 2 }).map((_, i) => (
              <CredentialCardSkeleton key={i} />
            ))}
          </div>
        </div>
      </Layout>
    );
  }

  if (error) {
    return (
      <Layout>
        <div className="text-center py-12">
          <p className="text-red-600 mb-4">{error}</p>
          <button
            onClick={() => navigate('/catalog')}
            className="text-indigo-600 hover:text-indigo-800"
          >
            Back to Catalog
          </button>
        </div>
      </Layout>
    );
  }

  return (
    <Layout>
      <div className="space-y-6">
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-bold text-gray-900">Receipt</h1>
            <p className="text-sm text-gray-500">
              Order: {orderId} &middot; {order?.product_name_at_purchase}
            </p>
          </div>
          <button
            onClick={handleDownloadAll}
            className="px-4 py-2 text-sm font-medium text-white bg-blue-600 rounded hover:bg-blue-700 transition-colors"
          >
            Download All
          </button>
        </div>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {credentials.map((cred, idx) => (
            <CredentialCard key={cred.id} credential={cred} index={idx} />
          ))}
        </div>
      </div>
    </Layout>
  );
}
