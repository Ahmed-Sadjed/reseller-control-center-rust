import { useState, useEffect, useCallback, useRef } from 'react';
import { useSearchParams } from 'react-router-dom';
import Layout from '../components/Layout';
import ProductCard from '../components/ProductCard';
import ProductCardSkeleton from '../components/skeletons/ProductCardSkeleton';
import { useToast } from '../context/ToastContext';
import api from '../lib/axios';

export default function ProductsPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const [products, setProducts] = useState([]);
  const [categories, setCategories] = useState([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState(searchParams.get('search') || '');
  const [categoryFilter, setCategoryFilter] = useState(searchParams.get('category') || '');
  const [page, setPage] = useState(parseInt(searchParams.get('page') || '1', 10));
  const [totalPages, setTotalPages] = useState(1);
  const { addToast } = useToast();
  const debounceRef = useRef(null);

  const fetchProducts = useCallback(async () => {
    setLoading(true);
    try {
      const params = {};
      const searchVal = searchParams.get('search');
      const categoryVal = searchParams.get('category');
      const pageVal = searchParams.get('page');
      if (searchVal) params.search = searchVal;
      if (categoryVal) params.category = categoryVal;
      if (pageVal) params.page = pageVal;
      const { data } = await api.get('/products/', { params });
      setProducts(data.results || data);
      if (data.count) {
        setPage(data.page || Math.ceil(data.count / 20));
      }
      if (data.total_pages) setTotalPages(data.total_pages);
    } catch {
      addToast('Failed to load products', 'error');
    } finally {
      setLoading(false);
    }
  }, [searchParams, addToast]);

  useEffect(() => { fetchProducts(); }, [fetchProducts]);

  useEffect(() => {
    api.get('/categories/').then(({ data }) => setCategories(data)).catch(() => {});
  }, []);

  const updateParams = (updates) => {
    const params = {};
    const s = searchParams.get('search') || search;
    const c = searchParams.get('category') || categoryFilter;
    if (s) params.search = s;
    if (c) params.category = c;
    if (updates.page) params.page = updates.page;
    setSearchParams(params);
  };

  const handleSearchChange = (e) => {
    const value = e.target.value;
    setSearch(value);
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => {
      const params = {};
      if (value) params.search = value;
      if (categoryFilter) params.category = categoryFilter;
      setSearchParams(params);
    }, 300);
  };

  const handleCategoryChange = (slug) => {
    setCategoryFilter(slug);
    const params = {};
    if (search) params.search = search;
    if (slug) params.category = slug;
    setSearchParams(params);
  };

  const clearFilters = () => {
    setSearch('');
    setCategoryFilter('');
    setSearchParams({});
  };

  const goToPage = (p) => {
    updateParams({ page: p });
  };

  const hasFilters = search || categoryFilter;

  return (
    <Layout>
      <div className="space-y-6">
        <h1 className="text-2xl font-bold text-gray-900">Products</h1>

        <div className="flex flex-col sm:flex-row gap-3 w-full">
          <div className="flex-1">
            <input
              type="text"
              value={search}
              onChange={handleSearchChange}
              placeholder="Search products..."
              className="w-full px-4 py-2 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-indigo-500"
            />
          </div>
          <select
            value={categoryFilter}
            onChange={(e) => handleCategoryChange(e.target.value)}
            className="px-4 py-2 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-indigo-500 bg-white"
          >
            <option value="">All Categories</option>
            {categories.map((cat) => (
              <option key={cat.id} value={cat.slug}>{cat.name}</option>
            ))}
          </select>
        </div>

        {hasFilters && (
          <div className="flex items-center gap-2">
            <span className="text-sm text-gray-500">Filters active:</span>
            {search && (
              <span className="px-2 py-1 text-xs bg-gray-100 rounded-full text-gray-700">
                &ldquo;{search}&rdquo;
              </span>
            )}
            {categoryFilter && (
              <span className="px-2 py-1 text-xs bg-gray-100 rounded-full text-gray-700">
                {categories.find((c) => c.slug === categoryFilter)?.name || categoryFilter}
              </span>
            )}
            <button
              onClick={clearFilters}
              className="text-xs text-red-600 hover:text-red-800 ml-2"
            >
              Clear all
            </button>
          </div>
        )}

        {loading ? (
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
            {Array.from({ length: 6 }).map((_, i) => (
              <ProductCardSkeleton key={i} />
            ))}
          </div>
        ) : products.length === 0 ? (
          <div className="text-center py-12">
            <p className="text-gray-500">No products found.</p>
            {hasFilters && (
              <button
                onClick={clearFilters}
                className="mt-2 text-sm text-indigo-600 hover:text-indigo-800"
              >
                Clear filters
              </button>
            )}
          </div>
        ) : (
          <>
            <p className="text-sm text-gray-500">{products.length} product{products.length !== 1 ? 's' : ''} found</p>
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
              {products.map((product) => (
                <ProductCard key={product.id} product={product} onError={(msg) => addToast(msg, 'error')} />
              ))}
            </div>
            {totalPages > 1 && (
              <div className="flex items-center justify-center gap-2 pt-4">
                <button
                  onClick={() => goToPage(page - 1)}
                  disabled={page <= 1}
                  className="px-3 py-1.5 text-sm border border-gray-300 rounded hover:bg-gray-50 disabled:opacity-40 disabled:cursor-not-allowed"
                >
                  Previous
                </button>
                {Array.from({ length: totalPages }, (_, i) => i + 1).map((p) => (
                  <button
                    key={p}
                    onClick={() => goToPage(p)}
                    className={`px-3 py-1.5 text-sm border rounded ${
                      p === page
                        ? 'bg-indigo-600 text-white border-indigo-600'
                        : 'border-gray-300 hover:bg-gray-50'
                    }`}
                  >
                    {p}
                  </button>
                ))}
                <button
                  onClick={() => goToPage(page + 1)}
                  disabled={page >= totalPages}
                  className="px-3 py-1.5 text-sm border border-gray-300 rounded hover:bg-gray-50 disabled:opacity-40 disabled:cursor-not-allowed"
                >
                  Next
                </button>
              </div>
            )}
          </>
        )}
      </div>
    </Layout>
  );
}
