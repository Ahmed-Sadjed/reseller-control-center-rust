import { useState, useEffect, useCallback } from 'react';
import { Link } from 'react-router-dom';
import Layout from '../components/Layout';
import StatsCard from '../components/StatsCard';
import CategoryCard from '../components/CategoryCard';
import CategoryCardSkeleton from '../components/skeletons/CategoryCardSkeleton';
import { useToast } from '../context/ToastContext';
import api from '../lib/axios';

export default function CatalogPage() {
  const [categories, setCategories] = useState([]);
  const [loading, setLoading] = useState(true);
  const [stats, setStats] = useState(null);
  const { addToast } = useToast();

  const fetchData = useCallback(async () => {
    try {
      const [catRes, statsRes] = await Promise.all([
        api.get('/categories/'),
        api.get('/stats/'),
      ]);
      setCategories(catRes.data);
      setStats(statsRes.data);
    } catch {
      addToast('Failed to load catalog data', 'error');
    } finally {
      setLoading(false);
    }
  }, [addToast]);

  useEffect(() => { fetchData(); }, [fetchData]);

  return (
    <Layout>
      <div className="space-y-6">
        <StatsCard stats={stats} />
        <div className="flex items-center justify-between">
          <h1 className="text-2xl font-bold text-gray-900">Catalog</h1>
          <Link
            to="/products"
            className="text-sm text-indigo-600 hover:text-indigo-800 font-medium"
          >
            Browse All Products &rarr;
          </Link>
        </div>
        {loading ? (
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-6">
            {Array.from({ length: 6 }).map((_, i) => (
              <CategoryCardSkeleton key={i} />
            ))}
          </div>
        ) : categories.length === 0 ? (
          <div className="text-center py-12 text-gray-500">No categories available.</div>
        ) : (
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-6">
            {categories.map((cat, i) => (
              <CategoryCard key={cat.id} category={cat} index={i} />
            ))}
          </div>
        )}
      </div>
    </Layout>
  );
}
