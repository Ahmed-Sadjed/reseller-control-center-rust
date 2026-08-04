import { useState, useEffect, useCallback } from 'react';
import { Link } from 'react-router-dom';
import Layout from '../components/Layout';
import api from '../lib/axios';

const ROWS = 5;

const statusColors = {
  PENDING: 'bg-yellow-100 text-yellow-800',
  COMPLETED: 'bg-green-100 text-green-800',
  FAILED: 'bg-red-100 text-red-800',
  REFUNDED: 'bg-gray-100 text-gray-800',
};

export default function OrdersHistory() {
  const [orders, setOrders] = useState([]);
  const [loading, setLoading] = useState(true);

  const fetchOrders = useCallback(async () => {
    try {
      const { data } = await api.get('/orders/');
      setOrders(data.results || data);
    } catch {
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchOrders();
  }, [fetchOrders]);

  return (
    <Layout>
      <div className="space-y-6">
        <h1 className="text-2xl font-bold text-gray-900">Order History</h1>
        {loading ? (
          <div className="bg-white shadow overflow-hidden rounded-lg">
            <div className="px-6 py-4 border-b border-gray-200 bg-gray-50">
              <div className="flex gap-8">
                <div className="h-3 w-16 bg-gray-200 animate-pulse rounded" />
                <div className="h-3 w-20 bg-gray-200 animate-pulse rounded" />
                <div className="h-3 w-10 bg-gray-200 animate-pulse rounded" />
                <div className="h-3 w-12 bg-gray-200 animate-pulse rounded" />
                <div className="h-3 w-14 bg-gray-200 animate-pulse rounded" />
                <div className="h-3 w-12 bg-gray-200 animate-pulse rounded" />
                <div className="flex-1" />
              </div>
            </div>
            <div className="divide-y divide-gray-200">
              {Array.from({ length: ROWS }).map((_, i) => (
                <div key={i} className="px-6 py-4 flex gap-8 items-center">
                  <div className="h-4 w-16 bg-gray-200 animate-pulse rounded" />
                  <div className="h-4 w-28 bg-gray-200 animate-pulse rounded" />
                  <div className="h-4 w-10 bg-gray-200 animate-pulse rounded" />
                  <div className="h-4 w-14 bg-gray-200 animate-pulse rounded" />
                  <div className="h-5 w-16 bg-gray-200 animate-pulse rounded-full" />
                  <div className="h-4 w-20 bg-gray-200 animate-pulse rounded" />
                  <div className="flex-1" />
                </div>
              ))}
            </div>
          </div>
        ) : orders.length === 0 ? (
          <div className="text-center py-12 text-gray-500">No orders yet.</div>
        ) : (
          <>
          <div className="block md:hidden space-y-3">
            {orders.map((order) => (
              <div key={order.id} className="bg-white rounded-lg shadow border border-gray-200 p-4">
                <div className="flex items-center justify-between mb-2">
                  <span className="text-xs font-mono text-gray-500">{order.uuid.substring(0, 8)}...</span>
                  <span className={`px-2 py-1 text-xs font-medium rounded-full ${statusColors[order.status] || 'bg-gray-100 text-gray-800'}`}>
                    {order.status}
                  </span>
                </div>
                <div className="space-y-1 text-sm">
                  <div className="flex justify-between">
                    <span className="text-gray-500">Product</span>
                    <span className="text-gray-900 text-right">{order.product_name_at_purchase}</span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-gray-500">Qty</span>
                    <span className="text-gray-900">{order.quantity}</span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-gray-500">Total</span>
                    <span className="text-gray-900">{order.total_credits}</span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-gray-500">Date</span>
                    <span className="text-gray-500">{new Date(order.created_at).toLocaleDateString()}</span>
                  </div>
                </div>
                {order.status === 'COMPLETED' && (
                  <div className="mt-3 pt-3 border-t border-gray-100">
                    <Link
                      to={`/receipt/${order.uuid}`}
                      className="text-indigo-600 hover:text-indigo-800 text-sm font-medium"
                    >
                      View Receipt &rarr;
                    </Link>
                  </div>
                )}
              </div>
            ))}
          </div>
          <div className="hidden md:block bg-white shadow overflow-hidden rounded-lg">
            <table className="min-w-full divide-y divide-gray-200">
              <thead className="bg-gray-50">
                <tr>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Order</th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Product</th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Qty</th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Total</th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Status</th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Date</th>
                  <th className="px-6 py-3"></th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-200">
                {orders.map((order) => (
                  <tr key={order.id} className="hover:bg-gray-50">
                    <td className="px-6 py-4 text-sm font-mono text-gray-900">
                      {order.uuid.substring(0, 8)}...
                    </td>
                    <td className="px-6 py-4 text-sm text-gray-900">{order.product_name_at_purchase}</td>
                    <td className="px-6 py-4 text-sm text-gray-900">{order.quantity}</td>
                    <td className="px-6 py-4 text-sm text-gray-900">{order.total_credits}</td>
                    <td className="px-6 py-4">
                      <span className={`px-2 py-1 text-xs font-medium rounded-full ${statusColors[order.status] || 'bg-gray-100 text-gray-800'}`}>
                        {order.status}
                      </span>
                    </td>
                    <td className="px-6 py-4 text-sm text-gray-500">
                      {new Date(order.created_at).toLocaleDateString()}
                    </td>
                    <td className="px-6 py-4 text-right">
                      {order.status === 'COMPLETED' && (
                        <Link
                          to={`/receipt/${order.uuid}`}
                          className="text-indigo-600 hover:text-indigo-800 text-sm font-medium"
                        >
                          View Receipt
                        </Link>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
        )}
      </div>
    </Layout>
  );
}
