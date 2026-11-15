import React, { useState, useEffect, useLayoutEffect, useRef, useMemo } from 'react';
import { createPortal } from 'react-dom';
import { AdminTableShell } from '@sdkwork/cloudroutes-pc-commons/components/AdminTableShell';
import { BottomPagination } from '@sdkwork/cloudroutes-pc-commons/components/BottomPagination';
import { BusinessStateTableRow } from '@sdkwork/cloudroutes-pc-commons/components/BusinessState';
import { ConfirmDialog } from '@sdkwork/cloudroutes-pc-commons/components/ConfirmDialog';
import { readMediaResourceUrl } from '@sdkwork/cloudroutes-pc-commons/media-resource';
import { Search, Plus, Cpu, X, Layers, Image as ImageIcon, MessageSquare, Headphones, ChevronRight, ChevronDown, Activity, Trash2, Edit, Music, Loader2, RefreshCw, Video, Volume2, Power, PowerOff, Globe2, ArrowRightLeft, Upload, Check } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { formatMoney } from '@sdkwork/utils/money';
import { ModelMappingService, ModelService, Vendor, Model, ModelMappingModelOption, ModelMappingRule, ModelMappingCreateInput, ModelMappingUpdateInput, ModelMappingBindingInput, ModelMappingRuleItemInput, KNOWN_VENDORS } from './modelService';
import { MODEL_PRICING_REGIONS, createModelInputFromForm, createVendorInputFromForm, updateModelInputFromForm } from './modelForm';
import { VendorPickerModal } from './vendorPickerModal';
import './adminCatalog.css';

type ModelModalityFilter = Model['type'];

type PricePopoverPosition = {
  left: number;
  maxHeight: number;
  top: number;
  width: number;
};

const PRICE_POPOVER_GAP = 8;
const PRICE_POPOVER_MAX_WIDTH = 480;
const PRICE_POPOVER_VIEWPORT_PADDING = 16;
const PRICE_POPOVER_Z_INDEX = 2_147_483_000;

type TranslationFunction = ReturnType<typeof useTranslation>['t'];

function ModelPricePopover({
  anchor,
  ariaLabel,
  children,
  className,
  onDismiss,
}: {
  anchor: HTMLButtonElement;
  ariaLabel: string;
  children: React.ReactNode;
  className: string;
  onDismiss: () => void;
}) {
  const popoverRef = useRef<HTMLDivElement | null>(null);
  const [position, setPosition] = useState<PricePopoverPosition | null>(null);

  useLayoutEffect(() => {
    const popover = popoverRef.current;
    if (!popover) {
      return undefined;
    }

    const updatePosition = () => {
      if (!anchor.isConnected) {
        onDismiss();
        return;
      }

      const anchorRect = anchor.getBoundingClientRect();
      const width = Math.min(
        PRICE_POPOVER_MAX_WIDTH,
        Math.max(0, window.innerWidth - PRICE_POPOVER_VIEWPORT_PADDING * 2),
      );
      const maxHeight = Math.max(0, window.innerHeight - PRICE_POPOVER_VIEWPORT_PADDING * 2);
      const renderedHeight = Math.min(popover.offsetHeight, maxHeight);
      const preferredLeft = anchorRect.right - width;
      const left = Math.min(
        Math.max(PRICE_POPOVER_VIEWPORT_PADDING, preferredLeft),
        window.innerWidth - width - PRICE_POPOVER_VIEWPORT_PADDING,
      );
      const belowTop = anchorRect.bottom + PRICE_POPOVER_GAP;
      const aboveTop = anchorRect.top - renderedHeight - PRICE_POPOVER_GAP;
      const fitsBelow = belowTop + renderedHeight <= window.innerHeight - PRICE_POPOVER_VIEWPORT_PADDING;
      const top = fitsBelow || aboveTop < PRICE_POPOVER_VIEWPORT_PADDING
        ? Math.min(belowTop, window.innerHeight - renderedHeight - PRICE_POPOVER_VIEWPORT_PADDING)
        : aboveTop;

      setPosition({
        left,
        maxHeight,
        top: Math.max(PRICE_POPOVER_VIEWPORT_PADDING, top),
        width,
      });
    };

    updatePosition();
    window.addEventListener('resize', updatePosition);
    document.addEventListener('scroll', updatePosition, true);
    return () => {
      window.removeEventListener('resize', updatePosition);
      document.removeEventListener('scroll', updatePosition, true);
    };
  }, [anchor, onDismiss]);

  useEffect(() => {
    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) {
        return;
      }
      if (popoverRef.current?.contains(target) || anchor.contains(target)) {
        return;
      }
      onDismiss();
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onDismiss();
        anchor.focus();
      }
    };

    document.addEventListener('pointerdown', handlePointerDown);
    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('pointerdown', handlePointerDown);
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [anchor, onDismiss]);

  return createPortal(
    <div
      ref={popoverRef}
      data-admin-model-price-popover
      className={className}
      role="dialog"
      aria-label={ariaLabel}
      style={{
        left: position?.left ?? 0,
        maxHeight: position?.maxHeight,
        top: position?.top ?? 0,
        visibility: position ? 'visible' : 'hidden',
        width: position?.width,
        zIndex: PRICE_POPOVER_Z_INDEX,
      }}
    >
      {children}
    </div>,
    document.body,
  );
}

export { FrontendModelAdmin } from './FrontendModelAdmin';

export function ModelAdmin() {
  const { t } = useTranslation();
  const [vendors, setVendors] = useState<Vendor[]>([]);
  const [models, setModels] = useState<Model[]>([]);
  const [vendorModelCounts, setVendorModelCounts] = useState<Record<string, number>>({});
  const [vendorModelTotal, setVendorModelTotal] = useState(0);
  const [selectedVendorId, setSelectedVendorId] = useState<string>('');
  const [search, setSearch] = useState('');
  const [modalityFilters, setModalityFilters] = useState<ModelModalityFilter[]>([]);
  const [isModalityFilterOpen, setIsModalityFilterOpen] = useState(false);
  const modalityFilterRef = useRef<HTMLDivElement | null>(null);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);

  const [isVendorModalOpen, setIsVendorModalOpen] = useState(false);
  const [isModelModalOpen, setIsModelModalOpen] = useState(false);
  const [isSyncing, setIsSyncing] = useState(false);
  const [syncNotice, setSyncNotice] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<Model | null>(null);
  const [editingModel, setEditingModel] = useState<Model | null>(null);
  const [deletingModelId, setDeletingModelId] = useState<string | null>(null);
  const [statusUpdatingModelId, setStatusUpdatingModelId] = useState<string | null>(null);
  const [selectedModality, setSelectedModality] = useState<Model['type']>('Chat');
  const [openPricePopoverModelId, setOpenPricePopoverModelId] = useState<string | null>(null);
  const [pricePopoverAnchor, setPricePopoverAnchor] = useState<HTMLButtonElement | null>(null);
  const [priceRegionByModelId, setPriceRegionByModelId] = useState<Record<string, string>>({});

  const [vendorSelection, setVendorSelection] = useState<string>('v_deepseek');
  const [vendorDesc, setVendorDesc] = useState<string>(KNOWN_VENDORS.find(v => v.id === 'v_deepseek')?.desc ?? '');
  const modelModalityFilterOptions: Array<{ value: ModelModalityFilter; label: string }> = [
    { value: 'Chat', label: t('admin.model.filters.llm') },
    { value: 'Image', label: t('admin.model.filters.image') },
    { value: 'Video', label: t('admin.model.filters.video') },
    { value: 'Audio', label: t('admin.model.filters.audio') },
    { value: 'SoundEffect', label: t('admin.model.filters.sfx') },
    { value: 'Music', label: t('admin.model.filters.music') },
    { value: 'Embedding', label: t('admin.model.filters.embedding') },
  ];

  const loadVendorModels = async () => {
    const vendor = vendors.find((entry) => entry.id === selectedVendorId);
    if (!vendor) {
      setModels([]);
      setVendorModelTotal(0);
      return;
    }
    setLoading(true);
    setLoadError(null);
    try {
      const pageResult = await ModelService.fetchModelsPage({
        vendorCode: vendor.vendorCode,
        q: search.trim() || undefined,
        modelTypes: modalityFilters.length > 0 ? modalityFilters.join(',') : undefined,
        page,
        pageSize,
      });
      setModels(pageResult.items);
      setVendorModelTotal(pageResult.totalCount);
      setVendorModelCounts((current) => ({
        ...current,
        [vendor.id]: pageResult.totalCount,
      }));
    } catch (error) {
      setLoadError(error instanceof Error ? error.message : 'Failed to load model catalog');
    } finally {
      setLoading(false);
    }
  };

  const loadInitialCatalog = async () => {
    setLoading(true);
    setLoadError(null);
    try {
      const initialized = await ModelService.fetchInitializedCatalog();
      setVendors(initialized.vendors);
      setVendorModelCounts({});
    } catch (error) {
      setLoadError(error instanceof Error ? error.message : 'Failed to load model catalog');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void loadInitialCatalog();
  }, []);

  useEffect(() => {
    if (!selectedVendorId || vendors.length === 0) {
      return;
    }
    void loadVendorModels();
  }, [vendors, selectedVendorId, page, pageSize, search, modalityFilters]);

  const selectedVendor = vendors.find(v => v.id === selectedVendorId);
  const paginatedVendorModels = models;

  useEffect(() => {
    setPage(1);
  }, [selectedVendorId, search, modalityFilters]);

  useEffect(() => {
    const maxPage = Math.max(1, Math.ceil(vendorModelTotal / pageSize));
    if (page > maxPage) {
      setPage(maxPage);
    }
  }, [page, pageSize, vendorModelTotal]);

  useEffect(() => {
    if (!isModalityFilterOpen) {
      return undefined;
    }

    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) {
        return;
      }
      if (modalityFilterRef.current && modalityFilterRef.current.contains(target)) {
        return;
      }
      setIsModalityFilterOpen(false);
    };

    document.addEventListener('pointerdown', handlePointerDown);
    return () => {
      document.removeEventListener('pointerdown', handlePointerDown);
    };
  }, [isModalityFilterOpen]);

  const openVendorModal = () => {
    setVendorSelection('v_deepseek');
    setVendorDesc(KNOWN_VENDORS.find(v => v.id === 'v_deepseek')?.desc ?? '');
    setIsVendorModalOpen(true);
  };

  const selectedModalityFilterLabels = modelModalityFilterOptions
    .filter(option => modalityFilters.includes(option.value))
    .map(option => option.label);
  const modalityFilterLabel = selectedModalityFilterLabels.length > 0
    ? selectedModalityFilterLabels.join(', ')
    : t('admin.model.filters.allModalities');
  const modelTableHeaderCellClassName = "sticky top-0 z-10 px-6 py-4 font-semibold";
  const modelPriceColumnClassName = `${modelTableHeaderCellClassName} min-w-[168px] whitespace-nowrap`;
  const modelPriceCellClassName = "relative px-6 py-4 min-w-[220px] whitespace-nowrap";
  const modelPriceSummaryButtonClassName = "inline-flex min-w-[176px] items-center justify-between gap-3 rounded-lg border border-slate-200 bg-slate-50 px-3 py-2 text-left text-xs text-slate-600 shadow-sm transition hover:border-indigo-200 hover:bg-indigo-50/70 dark:border-white/10 dark:bg-white/5 dark:text-slate-300 dark:hover:border-indigo-500/30 dark:hover:bg-indigo-500/10 whitespace-nowrap";
  const modelPricePopoverClassName = "fixed z-[2147483000] w-[480px] max-w-[calc(100vw-2rem)] overflow-y-auto rounded-lg border border-slate-200 bg-white opacity-100 shadow-2xl isolate dark:border-white/15 dark:bg-[#1a1a1a]";
  const dismissPricePopover = () => {
    setOpenPricePopoverModelId(null);
    setPricePopoverAnchor(null);
  };

  const toggleModalityFilter = (value: ModelModalityFilter) => {
    setPage(1);
    setModalityFilters(current => current.includes(value)
      ? current.filter(item => item !== value)
      : [...current, value],
    );
  };

  const handleSyncAll = async () => {
    if (isSyncing) return;
    if (!window.confirm(t('common.actions.syncModelCatalogConfirm'))) return;
    setIsSyncing(true);
    setLoadError(null);
    setSyncNotice(null);
    try {
      const report = await ModelService.syncVendorsAndModels();
      setSyncNotice(t('common.actions.syncModelCatalogDone', {
        version: report.catalogVersion || '—',
        vendors: report.vendorCount,
        models: report.modelCount,
        prices: report.priceCount,
      }));
      await loadInitialCatalog();
    } catch (error) {
      setLoadError(error instanceof Error ? error.message : 'Failed to sync model catalog');
    } finally {
      setIsSyncing(false);
    }
  };

  const handleAddVendor = async (e: React.FormEvent) => {
    e.preventDefault();
    setLoadError(null);
    const formData = new FormData(e.target as HTMLFormElement);
    const vendorInput = createVendorInputFromForm(formData, vendorSelection, KNOWN_VENDORS, vendorDesc);

    if (!vendorInput) return;

    try {
      const added = await ModelService.addVendor(vendorInput);
      setVendors([...vendors, added]);
      setIsVendorModalOpen(false);
      setSelectedVendorId(added.id);
    } catch (error) {
      setLoadError(error instanceof Error ? error.message : 'Failed to add model vendor');
    }
  };

  const handleAddModel = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!selectedVendor) return;
    setLoadError(null);
    const formData = new FormData(e.target as HTMLFormElement);
    try {
      if (editingModel) {
        const updated = await ModelService.updateModel(
          editingModel.id,
          updateModelInputFromForm(formData, selectedVendor.id, editingModel),
        );
        setModels(models.map(model => model.id === updated.id ? updated : model));
        setEditingModel(null);
        setIsModelModalOpen(false);
        return;
      }
      const added = await ModelService.addModel(createModelInputFromForm(formData, selectedVendor.id));
      setModels([...models, added]);
      setIsModelModalOpen(false);
    } catch (error) {
      setLoadError(error instanceof Error ? error.message : 'Failed to save model');
    }
  };

  const openAddModelModal = () => {
    setEditingModel(null);
    setSelectedModality('Chat');
    setIsModelModalOpen(true);
  };

  const openEditModelModal = (model: Model) => {
    setEditingModel(model);
    setSelectedModality(model.type);
    setIsModelModalOpen(true);
  };

  const closeModelModal = () => {
    setEditingModel(null);
    setIsModelModalOpen(false);
  };

  const closeDeleteConfirmation = () => {
    if (deletingModelId) {
      return;
    }
    setDeleteTarget(null);
  };

  const executeDeleteModel = async () => {
    if (!deleteTarget) {
      return;
    }
    const id = deleteTarget.id;
    setDeletingModelId(id);
    setLoadError(null);
    try {
      const success = await ModelService.deleteModel(id);
      if (success) {
        setModels(current => current.filter(m => m.id !== id));
      }
      setDeleteTarget(null);
    } catch (error) {
      setLoadError(error instanceof Error ? error.message : 'Failed to delete model');
    } finally {
      setDeletingModelId(null);
    }
  };

  const toggleModelStatus = async (model: Model) => {
    const nextStatus: Model['status'] = model.status === 'active' ? 'inactive' : 'active';
    setStatusUpdatingModelId(model.id);
    setLoadError(null);
    try {
      const updated = await ModelService.updateModelStatus(model.id, nextStatus);
      setModels(current => current.map(item => item.id === updated.id ? updated : item));
    } catch (error) {
      setLoadError(error instanceof Error ? error.message : 'Failed to update model status');
    } finally {
      setStatusUpdatingModelId(null);
    }
  };

  const getTypeIcon = (type: string) => {
    switch (type) {
      case 'Chat': return <MessageSquare className="w-3.5 h-3.5" />;
      case 'Image': return <ImageIcon className="w-3.5 h-3.5" />;
      case 'Audio': return <Headphones className="w-3.5 h-3.5" />;
      case 'Music': return <Music className="w-3.5 h-3.5" />;
      case 'SoundEffect': return <Volume2 className="w-3.5 h-3.5" />;
      case 'Video': return <Video className="w-3.5 h-3.5" />;
      case 'Embedding': return <Layers className="w-3.5 h-3.5" />;
      default: return <Cpu className="w-3.5 h-3.5" />;
    }
  };

  const getTypeLabel = (type: Model['type']) => t(modelTypeI18nKey(type));

  const getPriceRegionLabel = (regionCode: string): string => {
    const region = MODEL_PRICING_REGIONS.find(option => option.code === regionCode);
    return region ? t(region.labelKey) : regionCode;
  };

  const getModelRegionPrices = (model: Model) => {
    return model.regionPrices;
  };

  const getModelPriceSummary = (model: Model): string => {
    const regionPrices = getModelRegionPrices(model);
    const defaultPrice = regionPrices.find(price => price.regionCode === 'global') ?? regionPrices[0];
    if (!defaultPrice) {
      return `${t('admin.model.pricing.input')} ${formatPrice('', 'USD')} / ${t('admin.model.pricing.output')} ${formatPrice('', 'USD')}`;
    }
    return `${t('admin.model.pricing.input')} ${formatPrice(defaultPrice.priceIn, defaultPrice.currency)} / ${t('admin.model.pricing.output')} ${formatPrice(defaultPrice.priceOut, defaultPrice.currency)}`;
  };

  const formatContextTokens = (tokens: number | null) => {
    if (tokens === null || !Number.isFinite(tokens) || tokens <= 0) {
      return '-';
    }
    if (tokens >= 1_000_000) {
      return `${Number(tokens / 1_000_000).toLocaleString(undefined, { maximumFractionDigits: 1 })}M`;
    }
    if (tokens >= 1_000) {
      return `${Number(tokens / 1_000).toLocaleString(undefined, { maximumFractionDigits: 1 })}k`;
    }
    return tokens.toLocaleString();
  };

  const renderModalityParams = () => {
    const inputBaseCls = "w-full bg-white dark:bg-[#1a1a1a] border border-slate-300 dark:border-white/10 rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-indigo-500 dark:focus:border-indigo-500 focus:ring-1 focus:ring-indigo-500 text-slate-900 dark:text-white shadow-sm transition-all";
    const labelCls = "block text-xs font-medium text-slate-500 mb-1.5";

    const defaultCapability = selectedModality === 'Chat'
      ? { supportsStreaming: true, supportsTools: true, supportsJsonSchema: true }
      : { supportsStreaming: false, supportsTools: false, supportsJsonSchema: false };
    const supportsStreaming = editingModel?.supportsStreaming ?? defaultCapability.supportsStreaming;
    const supportsTools = editingModel?.supportsTools ?? defaultCapability.supportsTools;
    const supportsJsonSchema = editingModel?.supportsJsonSchema ?? defaultCapability.supportsJsonSchema;

    return (
      <div className="space-y-4 pt-4 border-t border-slate-200 dark:border-white/10">
        <h4 className="text-sm font-semibold text-slate-700 dark:text-slate-300">{t('admin.model.modelModal.capabilities')}</h4>
        <div className="grid grid-cols-2 gap-4">
          <div>
            <label className={labelCls}>{t('admin.model.modelModal.maxOutputTokens')}</label>
            <input name="maxOutputTokens" type="number" min="0" step="1" defaultValue={editingModel?.maxOutputTokens ?? ''} placeholder={t('admin.model.modelModal.optionalPlaceholder')} className={inputBaseCls} />
          </div>
          <div>
            <label className={labelCls}>{t('admin.model.modelModal.supportedLanguages')}</label>
            <input name="supportedLanguages" type="text" defaultValue={editingModel?.supportedLanguages.join(', ') ?? ''} placeholder={t('admin.model.modelModal.supportedLanguagesPlaceholder')} className={inputBaseCls} />
          </div>
        </div>
        <div>
          <label className={labelCls}>{t('admin.model.modelModal.description')}</label>
          <textarea name="description" rows={2} defaultValue={editingModel?.description ?? ''} placeholder={t('admin.model.modelModal.descriptionPlaceholder')} className={`${inputBaseCls} resize-none`} />
        </div>
        <div>
          <label className={labelCls}>{t('admin.model.modelModal.capabilityIntro')}</label>
          <textarea name="capabilityIntro" rows={2} defaultValue={editingModel?.capabilityIntro ?? ''} placeholder={t('admin.model.modelModal.capabilityIntroPlaceholder')} className={`${inputBaseCls} resize-none`} />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <div>
            <label className={labelCls}>{t('admin.model.modelModal.limitations')}</label>
            <textarea name="limitations" rows={2} defaultValue={editingModel?.limitations.join(', ') ?? ''} placeholder={t('admin.model.modelModal.limitationsPlaceholder')} className={`${inputBaseCls} resize-none`} />
          </div>
          <div>
            <label className={labelCls}>{t('admin.model.modelModal.useCases')}</label>
            <textarea name="useCases" rows={2} defaultValue={editingModel?.useCases.join(', ') ?? ''} placeholder={t('admin.model.modelModal.useCasesPlaceholder')} className={`${inputBaseCls} resize-none`} />
          </div>
        </div>
        <div className="grid grid-cols-3 gap-3">
          <label className="flex items-center gap-2 rounded-lg border border-slate-200 bg-white px-3 py-2 text-xs font-medium text-slate-600 shadow-sm dark:border-white/10 dark:bg-[#1a1a1a] dark:text-slate-300">
            <input name="supportsStreaming" type="checkbox" defaultChecked={supportsStreaming} className="h-4 w-4 rounded border-slate-300 text-indigo-600 focus:ring-indigo-500" />
            {t('admin.model.modelModal.supportsStreaming')}
          </label>
          <label className="flex items-center gap-2 rounded-lg border border-slate-200 bg-white px-3 py-2 text-xs font-medium text-slate-600 shadow-sm dark:border-white/10 dark:bg-[#1a1a1a] dark:text-slate-300">
            <input name="supportsTools" type="checkbox" defaultChecked={supportsTools} className="h-4 w-4 rounded border-slate-300 text-indigo-600 focus:ring-indigo-500" />
            {t('admin.model.modelModal.supportsTools')}
          </label>
          <label className="flex items-center gap-2 rounded-lg border border-slate-200 bg-white px-3 py-2 text-xs font-medium text-slate-600 shadow-sm dark:border-white/10 dark:bg-[#1a1a1a] dark:text-slate-300">
            <input name="supportsJsonSchema" type="checkbox" defaultChecked={supportsJsonSchema} className="h-4 w-4 rounded border-slate-300 text-indigo-600 focus:ring-indigo-500" />
            {t('admin.model.modelModal.supportsJsonSchema')}
          </label>
        </div>
      </div>
    );
  };

  const renderPricingPanel = () => {
    const priceInputClassName = "w-full bg-white dark:bg-[#1a1a1a] border border-slate-300 dark:border-white/10 rounded-lg pl-8 pr-3 py-2 text-sm focus:outline-none focus:border-indigo-500 dark:focus:border-indigo-500 focus:ring-1 focus:ring-indigo-500 text-slate-900 dark:text-white shadow-sm transition-all";
    const priceLabelClassName = "block text-xs font-medium text-slate-600 dark:text-slate-400 mb-1.5";
    const regionPriceForForm = (regionCode: string) => editingModel?.regionPrices.find(price => price.regionCode === regionCode);

    return (
      <aside className="rounded-xl border border-indigo-100 bg-indigo-50/50 p-4 dark:border-indigo-500/10 dark:bg-indigo-500/5">
        <div className="mb-4">
          <h4 className="text-sm font-semibold text-indigo-900 dark:text-indigo-300">{t('admin.model.modelModal.pricingRegionsTitle')}</h4>
          <div className="mt-1 text-xs font-medium text-indigo-700 dark:text-indigo-300">{t('admin.model.modelModal.pricingTitle')}</div>
          <p className="mt-1 text-xs leading-5 text-slate-500 dark:text-slate-400">{t('admin.model.modelModal.regionPricingHint')}</p>
        </div>
        <div className="space-y-4">
          {MODEL_PRICING_REGIONS.map((region) => {
            const regionPrice = regionPriceForForm(region.code);
            return (
            <section key={region.code} className="rounded-lg border border-slate-200 bg-white p-3 shadow-sm dark:border-white/10 dark:bg-[#121212]">
              <div className="mb-3 flex items-center justify-between">
                <div className="text-sm font-semibold text-slate-800 dark:text-slate-200">{t(region.labelKey)}</div>
                <div className="rounded-md bg-slate-100 px-2 py-0.5 font-mono text-[11px] text-slate-500 dark:bg-white/10 dark:text-slate-400">
                  {region.code}
                </div>
              </div>
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className={priceLabelClassName}>{t('admin.model.modelModal.inputUnitPrice')}</label>
                  <div className="relative">
                    <span className="absolute left-3 top-1/2 -translate-y-1/2 text-slate-400 font-mono">$</span>
                    <input required={region.code === 'global'} name={`priceIn.${region.code}`} type="number" step="0.000001" defaultValue={regionPrice?.priceIn ?? ''} placeholder="0.01" className={priceInputClassName} />
                  </div>
                </div>
                <div>
                  <label className={priceLabelClassName}>{t('admin.model.modelModal.outputUnitPrice')}</label>
                  <div className="relative">
                    <span className="absolute left-3 top-1/2 -translate-y-1/2 text-slate-400 font-mono">$</span>
                    <input required={region.code === 'global'} name={`priceOut.${region.code}`} type="number" step="0.000001" defaultValue={regionPrice?.priceOut ?? ''} placeholder="0.03" className={priceInputClassName} />
                  </div>
                </div>
                <div>
                  <label className={priceLabelClassName}>{t('admin.model.modelModal.cacheReadUnitPrice')}</label>
                  <div className="relative">
                    <span className="absolute left-3 top-1/2 -translate-y-1/2 text-slate-400 font-mono">$</span>
                    <input name={`cacheReadPrice.${region.code}`} type="number" step="0.000001" defaultValue={regionPrice?.cacheReadPrice ?? ''} placeholder="0.00" className={priceInputClassName} />
                  </div>
                </div>
                <div>
                  <label className={priceLabelClassName}>{t('admin.model.modelModal.cacheWriteUnitPrice')}</label>
                  <div className="relative">
                    <span className="absolute left-3 top-1/2 -translate-y-1/2 text-slate-400 font-mono">$</span>
                    <input name={`cacheWritePrice.${region.code}`} type="number" step="0.000001" defaultValue={regionPrice?.cacheWritePrice ?? ''} placeholder="0.00" className={priceInputClassName} />
                  </div>
                </div>
              </div>
            </section>
          );
          })}
        </div>
      </aside>
    );
  };

  return (
    <div className="flex min-h-0 h-full w-full flex-col bg-slate-50 dark:bg-[#121212] rounded-xl overflow-hidden shadow-sm border border-slate-200 dark:border-white/5">
      {syncNotice ? (
        <div className="shrink-0 border-b border-emerald-200 bg-emerald-50 px-4 py-2 text-sm text-emerald-700 dark:border-emerald-500/20 dark:bg-emerald-500/10 dark:text-emerald-300" role="status">
          {syncNotice}
        </div>
      ) : null}
      <div className="flex min-h-0 flex-1 overflow-hidden">
        {/* SIDEBAR - VENDORS */}
        <div className="w-64 bg-white dark:bg-[#1a1a1a] border-r border-slate-200 dark:border-white/10 flex flex-col shrink-0">
          <div className="border-b border-slate-200 bg-slate-50/50 p-4 dark:border-white/10 dark:bg-[#121212]/50">
            <div className="flex items-center justify-between gap-2">
              <span className="min-w-0 truncate text-sm font-semibold uppercase tracking-wider text-slate-700 dark:text-slate-300">{t('admin.model.vendorSidebar.title')}</span>
              <div className="flex shrink-0 items-center gap-1">
                <button
                  type="button"
                  onClick={handleSyncAll}
                  disabled={isSyncing}
                  className="flex shrink-0 items-center gap-1 rounded-md px-1.5 py-1 text-xs font-medium text-slate-500 transition-colors hover:bg-slate-100 hover:text-slate-700 disabled:cursor-not-allowed disabled:opacity-50 dark:hover:bg-white/10 dark:hover:text-slate-200"
                  title={isSyncing ? t('common.actions.syncingCatalog') : t('common.actions.syncModelCatalog')}
                  aria-label={isSyncing ? t('common.actions.syncingCatalog') : t('common.actions.syncModelCatalog')}
                  data-models-admin-catalog-sync
                >
                  {isSyncing ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <RefreshCw className="w-3.5 h-3.5" />}
                  <span className="hidden xl:inline">{isSyncing ? t('common.actions.syncingCatalog') : t('common.actions.syncModelCatalog')}</span>
                </button>
                <button type="button" onClick={openVendorModal} className="rounded-md p-1.5 text-slate-400 transition-colors hover:bg-indigo-50 hover:text-indigo-600 dark:hover:bg-indigo-500/10 dark:hover:text-indigo-400" title={t('common.actions.addModelVendor')}>
                  <Plus className="w-4 h-4" />
                </button>
              </div>
            </div>
          </div>
          <div className="flex-1 overflow-y-auto p-3 space-y-1.5">
            {vendors.map(v => {
              const isActive = selectedVendorId === v.id;
              const count = vendorModelCounts[v.id];
              const vendorAvatarAppearance = resolveVendorAvatarAppearance(v.color);
              return (
                <button
                  key={v.id}
                  onClick={() => setSelectedVendorId(v.id)}
                  className={`w-full flex items-center justify-between p-2.5 rounded-lg transition-all text-sm group ${
                    isActive
                    ? 'bg-indigo-50 dark:bg-indigo-500/10'
                    : 'hover:bg-slate-50 dark:hover:bg-white/5'
                  }`}
                >
                  <div className="flex min-w-0 flex-1 items-center gap-3">
                    <div
                      className={`w-6 h-6 rounded-md ${vendorAvatarAppearance.className} flex items-center justify-center text-white shadow-sm shrink-0 font-medium`}
                      style={vendorAvatarAppearance.style}
                    >
                       {v.name.charAt(0).toUpperCase()}
                    </div>
                    <span className={`min-w-0 truncate font-medium ${isActive ? 'text-indigo-700 dark:text-indigo-400' : 'text-slate-700 dark:text-slate-300'}`}>
                      {v.name}
                    </span>
                  </div>
                  <div className="flex shrink-0 items-center gap-2">
                    {typeof count === 'number' ? (
                      <span className={`text-xs px-2 py-0.5 rounded-full ${isActive ? 'bg-indigo-100 text-indigo-700 dark:bg-indigo-500/20 dark:text-indigo-300' : 'bg-slate-100 text-slate-500 dark:bg-white/10 dark:text-slate-400'}`}>
                        {count}
                      </span>
                    ) : null}
                  </div>
                </button>
              );
            })}
          </div>
        </div>

        {/* MAIN AREA - MODELS LIST */}
        <div className="flex min-h-0 flex-1 flex-col overflow-hidden bg-slate-50 p-4 dark:bg-[#121212]">
          {selectedVendor ? (
            <AdminTableShell
              data-admin-model-table-card
              className="flex-1 min-h-0 dark:bg-[#1a1a1a]"
              viewportClassName="min-h-0 flex-1"
              viewportProps={{ 'data-admin-model-table-viewport': true }}
              header={(
                <div className="border-b border-slate-200 p-3 dark:border-white/10">
                  <div className="flex flex-col gap-2 xl:flex-row xl:items-center xl:justify-between">
                    <div className="flex min-w-0 flex-col gap-2 md:flex-row md:items-center">
                      <div className="relative min-w-0 md:w-[320px]">
                        <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-slate-400" />
                        <input
                          type="text"
                          placeholder={t('admin.model.search.placeholder')}
                          value={search}
                          onChange={(event) => {
                            setPage(1);
                            setSearch(event.target.value);
                          }}
                          className="w-full rounded-lg border border-slate-200 bg-white py-2 pl-9 pr-3 text-sm text-slate-900 outline-none transition-colors placeholder:text-slate-400 focus:border-indigo-500 dark:border-white/10 dark:bg-white/5 dark:text-white"
                        />
                      </div>
                      <div ref={modalityFilterRef} className="relative" data-admin-model-modality-filter>
                        <button
                          type="button"
                          aria-label={t('admin.model.filters.modality')}
                          onClick={() => setIsModalityFilterOpen(current => !current)}
                          className="inline-flex h-10 max-w-[280px] items-center gap-2 rounded-lg border border-slate-200 bg-white px-3 text-sm text-slate-700 outline-none transition-colors hover:border-slate-300 focus:border-indigo-500 dark:border-white/10 dark:bg-[#202020] dark:text-slate-200 dark:hover:border-white/20"
                        >
                          <span className="truncate">
                            {modalityFilterLabel}
                          </span>
                          <ChevronDown className="h-4 w-4 shrink-0 text-slate-400" />
                        </button>
                        {isModalityFilterOpen ? (
                          <div className="absolute left-0 top-full z-30 mt-2 w-56 rounded-lg border border-slate-200 bg-white p-2 shadow-lg dark:border-white/10 dark:bg-[#202020]">
                            <div className="flex items-center justify-between border-b border-slate-100 px-2 pb-2 dark:border-white/10">
                              <span className="text-xs font-semibold uppercase tracking-wide text-slate-500 dark:text-slate-400">
                                {t('admin.model.filters.modality')}
                              </span>
                              {modalityFilters.length > 0 ? (
                                <button
                                  type="button"
                                  data-admin-model-modality-filter-clear
                                  onClick={() => {
                                    setPage(1);
                                    setModalityFilters([]);
                                  }}
                                  className="text-xs font-medium text-indigo-600 hover:text-indigo-700 dark:text-indigo-400"
                                >
                                  {t('common.actions.clear')}
                                </button>
                              ) : null}
                            </div>
                            <div className="mt-2 space-y-1">
                              {modelModalityFilterOptions.map((option) => (
                                <label
                                  key={option.value}
                                  data-admin-model-modality-filter-option
                                  className="flex cursor-pointer items-center gap-2 rounded-md px-2 py-2 text-sm text-slate-700 transition-colors hover:bg-slate-50 dark:text-slate-200 dark:hover:bg-white/5"
                                >
                                  <input
                                    type="checkbox"
                                    checked={modalityFilters.includes(option.value)}
                                    onChange={() => toggleModalityFilter(option.value)}
                                    className="h-4 w-4 rounded border-slate-300 text-indigo-600 focus:ring-indigo-500"
                                  />
                                  <span>{option.label}</span>
                                </label>
                              ))}
                            </div>
                          </div>
                        ) : null}
                      </div>
                    </div>
                    <button onClick={openAddModelModal} className="inline-flex w-fit items-center gap-2 rounded-lg bg-indigo-600 px-4 py-2 text-sm font-semibold text-white shadow-sm transition-colors hover:bg-indigo-700">
                      <Plus className="w-4 h-4" /> {t('common.actions.addModel')}
                    </button>
                  </div>
                </div>
              )}
              footer={(
                <div data-admin-model-pagination>
                  <BottomPagination
                    page={page}
                    pageSize={pageSize}
                    itemCount={paginatedVendorModels.length}
                    hasNextPage={page * pageSize < vendorModelTotal}
                    disabled={loading}
                    showingLabel={t('admin.model.pagination.showing')}
                    pageLabel={t('admin.model.pagination.page', { page })}
                    pageSizeLabel={t('admin.model.pagination.pageSize')}
                    previousLabel={t('common.actions.previousPage')}
                    nextLabel={t('common.actions.nextPage')}
                    onPreviousPage={() => setPage((current) => Math.max(1, current - 1))}
                    onNextPage={() => setPage((current) => current + 1)}
                    onPageSizeChange={(nextPageSize) => {
                      setPageSize(nextPageSize);
                      setPage(1);
                    }}
                  />
                </div>
              )}
            >
              <table className="w-full min-w-[960px] text-left text-sm text-slate-600 dark:text-slate-400 whitespace-nowrap">
                        <thead data-admin-model-table-header>
                          <tr>
                            <th className={modelTableHeaderCellClassName}>{t('admin.model.table.model')}</th>
                            <th className={modelTableHeaderCellClassName}>{t('admin.model.table.type')}</th>
                            <th className={modelPriceColumnClassName}>{t('admin.model.table.price')}</th>
                            <th className={modelTableHeaderCellClassName}>{t('admin.model.table.context')}</th>
                            <th className={modelTableHeaderCellClassName}>{t('admin.model.table.calls')}</th>
                            <th className={modelTableHeaderCellClassName}>{t('admin.model.table.status')}</th>
                            <th className={`${modelTableHeaderCellClassName} text-right`}>{t('admin.model.table.actions')}</th>
                          </tr>
                        </thead>
                        <tbody className="divide-y divide-slate-200 dark:divide-white/5">
                          {loading ? (
                            <BusinessStateTableRow colSpan={7} kind="loading" title={t('admin.model.state.loadingModels')} />
                          ) : loadError ? (
                            <BusinessStateTableRow
                              colSpan={7}
                              kind="error"
                              title={t('admin.model.state.modelsLoadError')}
                              description={loadError}
                              onRetry={() => { void loadVendorModels(); }}
                              retryLabel={t('common.actions.retry')}
                            />
                          ) : paginatedVendorModels.length === 0 ? (
                            <BusinessStateTableRow
                              colSpan={7}
                              kind="empty"
                              title={t('admin.model.state.noModels')}
                              description={t('admin.model.state.noModelsDescription')}
                              action={{
                                label: t('common.actions.addModel'),
                                onClick: openAddModelModal,
                              }}
                            />
                          ) : paginatedVendorModels.map(m => (
                            <tr key={m.id} className="hover:bg-slate-50/80 dark:hover:bg-white/[0.02] transition-colors group">
                              <td className="px-6 py-4">
                                <div className="min-w-0">
                                  <div className="font-semibold text-slate-900 dark:text-white flex items-center gap-2.5">
                                    <div className="w-2 h-2 rounded-full bg-indigo-500 shadow-[0_0_8px_rgba(99,102,241,0.5)]" />
                                    <span className="truncate">{m.displayName}</span>
                                  </div>
                                  {m.displayName !== m.model ? (
                                    <div className="ml-4 mt-1 max-w-[280px] truncate font-mono text-xs text-slate-400 dark:text-slate-500">
                                      {m.model}
                                    </div>
                                  ) : null}
                                </div>
                              </td>
                              <td className="px-6 py-4">
                                <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-slate-100 text-slate-600 dark:bg-white/10 dark:text-slate-300 text-xs font-medium">
                                  {getTypeIcon(m.type)} {getTypeLabel(m.type)}
                                </span>
                              </td>
                              <td className={modelPriceCellClassName}>
                                {(() => {
                                  const regionPrices = getModelRegionPrices(m);
                                  const defaultPriceRegionCode = regionPrices.find(price => price.regionCode === 'global')?.regionCode ?? regionPrices[0]?.regionCode ?? 'global';
                                  const selectedPriceRegionCode = priceRegionByModelId[m.id] ?? defaultPriceRegionCode;
                                  const selectedPriceRegion = regionPrices.find(price => price.regionCode === selectedPriceRegionCode) ?? regionPrices[0];
                                  const selectedPriceCurrency = selectedPriceRegion?.currency ?? 'USD';
                                  const priceRows = [
                                    { label: t('admin.model.pricing.input'), value: selectedPriceRegion?.priceIn },
                                    { label: t('admin.model.pricing.output'), value: selectedPriceRegion?.priceOut },
                                    { label: t('admin.model.pricing.cacheRead'), value: selectedPriceRegion?.cacheReadPrice },
                                    { label: t('admin.model.pricing.cacheWrite'), value: selectedPriceRegion?.cacheWritePrice },
                                  ];

                                  return (
                                    <div className="relative inline-flex">
                                      <button
                                        type="button"
                                        data-admin-model-price-summary={m.id}
                                        className={modelPriceSummaryButtonClassName}
                                        aria-haspopup="dialog"
                                        aria-expanded={openPricePopoverModelId === m.id}
                                        onClick={(event) => {
                                          const isOpen = openPricePopoverModelId === m.id;
                                          setOpenPricePopoverModelId(isOpen ? null : m.id);
                                          setPricePopoverAnchor(isOpen ? null : event.currentTarget);
                                        }}
                                      >
                                        <span className="min-w-0">
                                          <span className="block max-w-[190px] truncate font-mono text-[11px] text-slate-900 dark:text-slate-100">
                                            {getModelPriceSummary(m)}
                                          </span>
                                          <span className="mt-0.5 block text-[11px] text-slate-500 dark:text-slate-400">
                                            {regionPrices.length > 1
                                              ? t('admin.model.pricing.regionCount', { count: regionPrices.length })
                                              : selectedPriceRegion ? getPriceRegionLabel(selectedPriceRegion.regionCode) : '-'}
                                          </span>
                                        </span>
                                        <ChevronDown className={`h-3.5 w-3.5 shrink-0 transition-transform ${openPricePopoverModelId === m.id ? 'rotate-180' : ''}`} />
                                      </button>

                                      {openPricePopoverModelId === m.id && pricePopoverAnchor ? (
                                        <ModelPricePopover
                                          key={m.id}
                                          anchor={pricePopoverAnchor}
                                          ariaLabel={t('admin.model.pricing.details')}
                                          className={modelPricePopoverClassName}
                                          onDismiss={dismissPricePopover}
                                        >
                                          <div className="border-b border-slate-200 px-3 py-2 dark:border-white/10">
                                            <div className="flex items-center justify-between gap-3">
                                              <div className="text-xs font-semibold text-slate-800 dark:text-slate-100">{t('admin.model.pricing.details')}</div>
                                              <div className="max-w-[170px] truncate font-mono text-[11px] text-slate-400">{m.model}</div>
                                            </div>
                                            {regionPrices.length > 1 ? (
                                              <div className="mt-2 flex gap-1 overflow-x-auto" role="tablist" aria-label={t('admin.model.pricing.details')}>
                                                {regionPrices.map(regionPrice => {
                                                  const isSelected = regionPrice.regionCode === selectedPriceRegion?.regionCode;
                                                  return (
                                                    <button
                                                      key={regionPrice.regionCode}
                                                      type="button"
                                                      role="tab"
                                                      aria-selected={isSelected}
                                                      className={`shrink-0 rounded-md px-2.5 py-1 text-xs font-medium transition ${
                                                        isSelected
                                                          ? 'bg-indigo-600 text-white shadow-sm'
                                                          : 'bg-slate-100 text-slate-600 hover:bg-slate-200 dark:bg-white/10 dark:text-slate-300 dark:hover:bg-white/15'
                                                      }`}
                                                      onClick={() => setPriceRegionByModelId(current => ({ ...current, [m.id]: regionPrice.regionCode }))}
                                                    >
                                                      {getPriceRegionLabel(regionPrice.regionCode)}
                                                    </button>
                                                  );
                                                })}
                                              </div>
                                            ) : null}
                                          </div>
                                          <div className="p-3">
                                            <div className="mb-2 flex items-center justify-between gap-3">
                                              <span className="text-xs font-medium text-slate-600 dark:text-slate-300">{selectedPriceRegion ? getPriceRegionLabel(selectedPriceRegion.regionCode) : '-'}</span>
                                              <span className="rounded bg-slate-100 px-2 py-0.5 font-mono text-[11px] text-slate-500 dark:bg-white/10 dark:text-slate-400">
                                                {selectedPriceRegion?.regionCode ?? '-'}
                                              </span>
                                            </div>
                                            <div className="grid gap-1.5">
                                              {priceRows.map(row => (
                                                <div key={row.label} className="flex items-center justify-between gap-3 rounded-md bg-slate-50 px-2.5 py-1.5 text-xs dark:bg-white/5">
                                                  <span className="text-slate-500 dark:text-slate-400">{row.label}</span>
                                                  <span className="font-mono text-slate-900 dark:text-slate-100">{formatPrice(row.value ?? '', selectedPriceCurrency)}</span>
                                                </div>
                                              ))}
                                            </div>
                                          </div>
                                        </ModelPricePopover>
                                      ) : null}
                                    </div>
                                  );
                                })()}
                              </td>
                              <td className="px-6 py-4">
                                <div className="inline-flex px-2 py-1 text-xs font-mono bg-slate-50 border border-slate-200 text-slate-600 dark:bg-[#1a1a1a] dark:text-slate-400 dark:border-white/10 rounded-md">
                                  {formatContextTokens(m.contextTokens)}
                                </div>
                              </td>
                              <td className="px-6 py-4">
                                <div className="flex items-center gap-1.5 text-slate-600 dark:text-slate-400 font-mono text-sm">
                                  <Activity className="w-3.5 h-3.5 text-emerald-500" /> {m.calls}
                                </div>
                              </td>
                              <td className="px-6 py-4">
                                {m.status === 'active' ? (
                                  <span className="inline-flex items-center gap-1.5 px-2 py-1 rounded-md bg-emerald-50 text-emerald-600 dark:bg-emerald-500/10 dark:text-emerald-400 text-xs font-medium border border-emerald-200/50 dark:border-emerald-500/20">
                                    <span className="w-1.5 h-1.5 rounded-full bg-emerald-500 animate-pulse" /> {t('admin.model.status.active')}
                                  </span>
                                ) : (
                                  <span className="inline-flex items-center gap-1.5 px-2 py-1 rounded-md bg-slate-100 text-slate-600 dark:bg-white/10 dark:text-slate-400 text-xs font-medium border border-slate-200 dark:border-white/10">
                                    <span className="w-1.5 h-1.5 rounded-full bg-slate-400" /> {t('admin.model.status.inactive')}
                                  </span>
                                )}
                              </td>
                              <td className="px-6 py-4 text-right">
                                <div className="flex items-center justify-end gap-1">
                                  <button
                                    onClick={() => { void toggleModelStatus(m); }}
                                    disabled={statusUpdatingModelId === m.id}
                                    className={m.status === 'active'
                                      ? "p-2 text-slate-400 hover:text-amber-600 hover:bg-amber-50 disabled:cursor-not-allowed disabled:opacity-60 dark:hover:text-amber-400 dark:hover:bg-amber-500/10 rounded-lg transition-colors"
                                      : "p-2 text-slate-400 hover:text-emerald-600 hover:bg-emerald-50 disabled:cursor-not-allowed disabled:opacity-60 dark:hover:text-emerald-400 dark:hover:bg-emerald-500/10 rounded-lg transition-colors"}
                                    title={m.status === 'active' ? t('common.actions.disable') : t('common.actions.enable')}
                                  >
                                    {statusUpdatingModelId === m.id ? (
                                      <Loader2 className="w-4 h-4 animate-spin" />
                                    ) : m.status === 'active' ? (
                                      <PowerOff className="w-4 h-4" />
                                    ) : (
                                      <Power className="w-4 h-4" />
                                    )}
                                  </button>
                                  <button onClick={() => openEditModelModal(m)} className="p-2 text-slate-400 hover:text-indigo-600 hover:bg-indigo-50 dark:hover:text-indigo-400 dark:hover:bg-indigo-500/10 rounded-lg transition-colors" title={t('common.actions.edit')}>
                                    <Edit className="w-4 h-4" />
                                  </button>
                                  <button onClick={() => setDeleteTarget(m)} className="p-2 text-slate-400 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-500/10 rounded-lg transition-colors" title={t('common.actions.delete')}>
                                    <Trash2 className="w-4 h-4" />
                                  </button>
                                </div>
                              </td>
                            </tr>
                          ))}
                        </tbody>
              </table>
                 </AdminTableShell>
          ) : (
             <div className="flex-1 flex items-center justify-center flex-col text-slate-400">
               <Layers className="w-12 h-12 mb-4 text-slate-300 dark:text-slate-600" />
               <p>{t('admin.model.state.selectVendor')}</p>
             </div>
          )}
        </div>
      </div>

      {/* ADD VENDOR MODAL */}
      {isVendorModalOpen && (
        <div className="fixed inset-0 z-[60] flex items-center justify-center p-4 bg-slate-900/50 backdrop-blur-sm">
          <div className="bg-white dark:bg-[#1a1a1a] border border-slate-200 dark:border-white/10 rounded-2xl shadow-xl w-full max-w-md overflow-hidden flex flex-col">
            <div className="flex justify-between items-center p-5 border-b border-slate-200 dark:border-white/10">
              <h3 className="text-lg font-bold text-slate-900 dark:text-white">{t('admin.model.vendorModal.title')}</h3>
              <button onClick={() => setIsVendorModalOpen(false)} className="text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 transition-colors">
                <X className="w-5 h-5" />
              </button>
            </div>
            <form onSubmit={handleAddVendor} className="flex flex-col">
              <div className="p-5 space-y-5">
                <div>
                  <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">{t('admin.model.vendorModal.vendorBrand')}</label>
                  <div className="relative mb-3">
                    <select
                      value={vendorSelection}
                      onChange={e => {
                        setVendorSelection(e.target.value);
                        const found = KNOWN_VENDORS.find(v => v.id === e.target.value);
                        if(found && e.target.value !== 'custom') {
                          setVendorDesc(found.desc);
                        } else {
                          setVendorDesc('');
                        }
                      }}
                      className="w-full bg-slate-50 dark:bg-[#121212] border border-slate-300 dark:border-white/10 rounded-xl pl-4 pr-10 py-2.5 text-sm focus:outline-none focus:border-indigo-500 dark:focus:border-indigo-500 focus:ring-1 focus:ring-indigo-500 text-slate-900 dark:text-white shadow-sm transition-all appearance-none cursor-pointer"
                    >
                      <option value="" disabled>{t('admin.model.vendorModal.selectPlaceholder')}</option>
                      {KNOWN_VENDORS.map(v => (
                        <option key={v.id} value={v.id}>{v.name}</option>
                      ))}
                    </select>
                    <ChevronRight className="w-4 h-4 absolute right-3 top-1/2 -translate-y-1/2 text-slate-400 pointer-events-none rotate-90" />
                  </div>
                  {vendorSelection === 'custom' && (
                    <input required name="customName" type="text" placeholder={t('admin.model.vendorModal.customNamePlaceholder')} className="w-full bg-slate-50 dark:bg-[#121212] border border-slate-300 dark:border-white/10 rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:border-indigo-500 dark:focus:border-indigo-500 focus:ring-1 focus:ring-indigo-500 text-slate-900 dark:text-white shadow-sm transition-all animate-in fade-in slide-in-from-top-2" />
                  )}
                </div>
                <div>
                  <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">{t('admin.model.vendorModal.description')}</label>
                  <textarea name="description" value={vendorDesc} onChange={e => setVendorDesc(e.target.value)} rows={3} placeholder={t('admin.model.vendorModal.descriptionPlaceholder')} className="w-full bg-slate-50 dark:bg-[#121212] border border-slate-300 dark:border-white/10 rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:border-indigo-500 dark:focus:border-indigo-500 focus:ring-1 focus:ring-indigo-500 text-slate-900 dark:text-white shadow-sm transition-all resize-none"></textarea>
                </div>
              </div>
              <div className="p-5 border-t border-slate-200 dark:border-white/10 flex justify-end gap-3 bg-slate-50 dark:bg-[#121212]">
                <button type="button" onClick={() => setIsVendorModalOpen(false)} className="px-5 py-2.5 text-sm font-medium text-slate-700 dark:text-slate-300 hover:bg-slate-200 dark:hover:bg-white/10 rounded-xl transition-colors border border-slate-300 dark:border-slate-700 bg-white dark:bg-[#1a1a1a]">
                  {t('common.actions.cancel')}
                </button>
                <button type="submit" className="px-5 py-2.5 text-sm font-medium text-white bg-indigo-600 hover:bg-indigo-700 rounded-xl shadow-sm transition-colors border border-transparent">
                  {t('common.actions.addModelVendor')}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* ADD MODEL MODAL */}
      {isModelModalOpen && selectedVendor && (
        <div className="fixed inset-0 z-[60] flex items-center justify-center p-4 bg-slate-900/50 backdrop-blur-sm shadow-2xl">
          <div className="bg-white dark:bg-[#1a1a1a] border border-slate-200 dark:border-white/10 rounded-2xl shadow-2xl w-full max-w-5xl overflow-hidden flex flex-col">
            <div className="flex justify-between items-center p-5 border-b border-slate-200 dark:border-white/10">
              <h3 className="text-lg font-bold text-slate-900 dark:text-white flex items-center gap-2">
                 <Plus className="w-5 h-5 text-indigo-500" />
                 {editingModel ? t('admin.model.modelModal.editTitle') : t('admin.model.modelModal.connectTitle')} <span className="text-sm font-normal text-slate-500 dark:text-slate-400">{selectedVendor.name}</span>
              </h3>
              <button onClick={closeModelModal} className="text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 transition-colors">
                <X className="w-5 h-5" />
              </button>
            </div>
            <form onSubmit={handleAddModel} className="flex flex-col">
              <div className="grid max-h-[calc(100vh-12rem)] grid-cols-1 gap-6 overflow-y-auto p-6 lg:grid-cols-[minmax(0,1fr)_360px]">
                <div className="space-y-6">
                  <div className="grid grid-cols-2 gap-5">
                    <div>
                      <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">{t('admin.model.modelModal.modelId')}</label>
                      <input required name="model" type="text" defaultValue={editingModel?.model ?? ''} placeholder={t('admin.model.modelModal.modelIdPlaceholder')} className="w-full bg-slate-50 dark:bg-[#121212] border border-slate-300 dark:border-white/10 rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:border-indigo-500 dark:focus:border-indigo-500 focus:ring-1 focus:ring-indigo-500 text-slate-900 dark:text-white shadow-sm transition-all" />
                    </div>
                    <div>
                      <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">{t('admin.model.modelModal.displayName')}</label>
                      <input name="displayName" type="text" defaultValue={editingModel && editingModel.displayName !== editingModel.model ? editingModel.displayName : ''} placeholder={t('admin.model.modelModal.displayNamePlaceholder')} className="w-full bg-slate-50 dark:bg-[#121212] border border-slate-300 dark:border-white/10 rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:border-indigo-500 dark:focus:border-indigo-500 focus:ring-1 focus:ring-indigo-500 text-slate-900 dark:text-white shadow-sm transition-all" />
                    </div>
                  </div>

                  <div className="grid grid-cols-2 gap-5">
                    <div>
                      <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">{t('admin.model.modelModal.modelType')}</label>
                      <div className="relative">
                        <select required name="type" value={selectedModality} onChange={e => setSelectedModality(e.target.value as Model['type'])} className="w-full bg-slate-50 dark:bg-[#121212] border border-slate-300 dark:border-white/10 rounded-xl pl-4 pr-10 py-2.5 text-sm focus:outline-none focus:border-indigo-500 dark:focus:border-indigo-500 focus:ring-1 focus:ring-indigo-500 text-slate-900 dark:text-white shadow-sm transition-all appearance-none cursor-pointer">
                          <option value="Video">{t('admin.model.modelTypes.video')}</option>
                          <option value="Chat">{t('admin.model.modelTypes.chat')}</option>
                          <option value="Image">{t('admin.model.modelTypes.image')}</option>
                          <option value="Audio">{t('admin.model.modelTypes.audio')}</option>
                          <option value="Music">{t('admin.model.modelTypes.music')}</option>
                          <option value="SoundEffect">{t('admin.model.modelTypes.soundEffect')}</option>
                          <option value="Embedding">{t('admin.model.modelTypes.embedding')}</option>
                        </select>
                        <ChevronRight className="w-4 h-4 absolute right-3 top-1/2 -translate-y-1/2 text-slate-400 pointer-events-none rotate-90" />
                      </div>
                    </div>
                    <div>
                      <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">{t('admin.model.modelModal.contextWindow')}</label>
                      <input name="contextTokens" type="text" defaultValue={editingModel ? String(editingModel.contextTokens) : ''} placeholder={t('admin.model.modelModal.contextPlaceholder')} className="w-full bg-slate-50 dark:bg-[#121212] border border-slate-300 dark:border-white/10 rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:border-indigo-500 dark:focus:border-indigo-500 focus:ring-1 focus:ring-indigo-500 text-slate-900 dark:text-white shadow-sm transition-all" />
                    </div>
                  </div>

                  {renderModalityParams()}
                </div>

                {renderPricingPanel()}
              </div>

              <div className="p-5 border-t border-slate-200 dark:border-white/10 flex justify-end gap-3 bg-slate-50 dark:bg-[#121212] rounded-b-2xl">
                <button type="button" onClick={closeModelModal} className="px-5 py-2.5 text-sm font-medium text-slate-700 dark:text-slate-300 hover:bg-slate-200 dark:hover:bg-white/10 rounded-xl transition-colors border border-slate-300 dark:border-slate-700 bg-white dark:bg-[#1a1a1a]">
                  {t('common.actions.cancel')}
                </button>
                <button type="submit" className="px-5 py-2.5 text-sm font-medium text-white bg-indigo-600 hover:bg-indigo-700 rounded-xl shadow-sm transition-colors border border-transparent">
                  {editingModel ? t('common.actions.saveModelChanges') : t('common.actions.confirmAndEnableModel')}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {deleteTarget && (
        <ConfirmDialog
          title={t('admin.model.delete.title')}
          description={t('admin.model.delete.description', { name: deleteTarget.displayName })}
          confirmLabel={t('admin.model.delete.confirm')}
          tone="danger"
          icon={<Trash2 className="h-4 w-4" />}
          isBusy={deletingModelId === deleteTarget.id}
          onConfirm={() => void executeDeleteModel()}
          onCancel={closeDeleteConfirmation}
        />
      )}
    </div>
  );
}

function modelsForVendor(models: readonly Model[], vendor: Vendor): Model[] {
  return models.filter((model) => model.vendorId === vendor.id || model.vendorCode === vendor.vendorCode);
}

function resolveVendorAvatarAppearance(color: string): { className: string; style?: React.CSSProperties } {
  const normalized = color.trim();
  if (/^#[0-9a-f]{3,8}$/iu.test(normalized)) {
    return { className: '', style: { backgroundColor: normalized } };
  }
  return { className: normalized.startsWith('bg-') ? normalized : 'bg-slate-700' };
}

function formatPrice(value: string, currency: string): string {
  const normalized = value.trim();
  if (!normalized) {
    return '-';
  }
  const normalizedCurrency = currency.trim().toUpperCase();
  const formatted = formatMoney(normalized, {
    currency: normalizedCurrency,
    locale: 'en-US',
    mode: 'symbol',
    minFractionDigits: 0,
    maxFractionDigits: 6,
  });
  if (formatted !== null) {
    return formatted;
  }
  return `${normalizedCurrency || 'USD'} ${normalized}`;
}

function modelTypeI18nKey(type: Model['type']): string {
  switch (type) {
    case 'Video':
      return 'admin.model.modelTypes.video';
    case 'Chat':
      return 'admin.model.modelTypes.chat';
    case 'Image':
      return 'admin.model.modelTypes.image';
    case 'Audio':
      return 'admin.model.modelTypes.audio';
    case 'Music':
      return 'admin.model.modelTypes.music';
    case 'SoundEffect':
      return 'admin.model.modelTypes.soundEffect';
    case 'Embedding':
      return 'admin.model.modelTypes.embedding';
  }
}

type ModelMappingBindingFilter = ModelMappingRule['bindingType'] | 'all';

type ModelMappingRowDraft = {
  id: string;
  persistedId: string | null;
  sourceModel: string;
  targetModel: string;
  enabled: boolean;
};

type ModelMappingBindingDraft = {
  id: string;
  persistedId: string | null;
  bindingType: ModelMappingRule['bindingType'];
  bindingId: string | null;
  bindingCode: string;
  bindingName: string;
  enabled: boolean;
};

type ModelMappingFieldErrorKey = 'sourceVendorCode' | 'targetVendorCode' | 'channelCode' | 'mappingRows' | 'mappingBindings';
type ModelMappingRowFieldKey = 'sourceModel' | 'targetModel';
type ModelMappingRowErrors = Partial<Record<ModelMappingRowFieldKey, string>>;
type ModelMappingFormErrors = {
  message: string;
  fieldErrors: Partial<Record<ModelMappingFieldErrorKey, string>>;
  rowErrors: Record<string, ModelMappingRowErrors>;
  firstErrorKey: string | null;
};
const MODEL_MAPPING_MAX_ROWS = 100;
const MODEL_MAPPING_MODEL_VALUE_MAX_LENGTH = 512;
let nextModelMappingDraftSequence = 0;

export function ModelMappingAdmin() {
  const { t } = useTranslation();
  const [mappings, setMappings] = useState<ModelMappingRule[]>([]);
  const [vendors, setVendors] = useState<Vendor[]>([]);
  const [bindingFilter, setBindingFilter] = useState<ModelMappingBindingFilter>('global');
  const [search, setSearch] = useState('');
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [catalogError, setCatalogError] = useState<string | null>(null);
  const [editorError, setEditorError] = useState<ModelMappingFormErrors | null>(null);
  const [editingMapping, setEditingMapping] = useState<ModelMappingRule | null>(null);
  const [editingRelationMapping, setEditingRelationMapping] = useState<ModelMappingRule | null>(null);
  const [isEditorOpen, setIsEditorOpen] = useState(false);
  const mappingsLoadSeqRef = useRef(0);

  const loadMappings = async (nextBindingFilter: ModelMappingBindingFilter = bindingFilter) => {
    const requestSeq = mappingsLoadSeqRef.current + 1;
    mappingsLoadSeqRef.current = requestSeq;
    setLoading(true);
    setLoadError(null);
    try {
      const items = await ModelMappingService.fetchMappings({
        bindingType: nextBindingFilter,
        q: search.trim() || null,
      });
      if (mappingsLoadSeqRef.current !== requestSeq) {
        return;
      }
      setMappings(items);
    } catch (error) {
      if (mappingsLoadSeqRef.current !== requestSeq) {
        return;
      }
      setLoadError(error instanceof Error ? error.message : t('admin.model.mapping.errors.loadMappings'));
    } finally {
      if (mappingsLoadSeqRef.current === requestSeq) {
        setLoading(false);
      }
    }
  };

  useEffect(() => {
    void loadMappings('global');
  }, []);

  const loadCatalog = async () => {
    setCatalogError(null);
    try {
      setVendors(await ModelService.fetchVendors());
    } catch (error) {
      setCatalogError(error instanceof Error ? error.message : t('admin.model.mapping.errors.loadCatalog'));
    }
  };

  useEffect(() => {
    void loadCatalog();
  }, []);

  const filteredMappings = mappings.filter((mapping) => {
    const query = search.trim().toLowerCase();
    if (!query) {
      return true;
    }
    return [
      mapping.sourceVendorCode,
      mapping.targetVendorCode,
      ...mapping.bindings.flatMap((binding) => [binding.bindingType, binding.bindingCode, binding.bindingName]),
      ...mapping.mappingItems.flatMap((item) => [item.sourceModel, item.targetModel]),
    ].some((value) => (value ?? '').toLowerCase().includes(query));
  });

  const openCreateMapping = () => {
    setEditingMapping(null);
    setEditorError(null);
    setLoadError(null);
    setIsEditorOpen(true);
  };

  const openCreateMappingWithBinding = (bindingType: ModelMappingRule['bindingType']) => {
    setBindingFilter(bindingType);
    openCreateMapping();
  };

  const openEditMapping = (mapping: ModelMappingRule) => {
    setEditingMapping(mapping);
    setEditorError(null);
    setIsEditorOpen(true);
  };

  const openRelationEditor = (mapping: ModelMappingRule) => {
    setEditorError(null);
    setEditingRelationMapping(mapping);
  };

  const handleBindingFilterChange = (nextBindingFilter: ModelMappingBindingFilter) => {
    setBindingFilter(nextBindingFilter);
    void loadMappings(nextBindingFilter);
  };

  const closeEditor = () => {
    if (saving) {
      return;
    }
    setIsEditorOpen(false);
    setEditingMapping(null);
    setEditorError(null);
  };

  const clearEditorFieldError = (field: ModelMappingFieldErrorKey) => {
    setEditorError((current) => clearModelMappingFormFieldError(current, field, t));
  };

  const clearEditorRowError = (rowId: string, field: ModelMappingRowFieldKey) => {
    setEditorError((current) => clearModelMappingFormRowError(current, rowId, field, t));
  };

  const handleSaveMapping = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setSaving(true);
    setLoadError(null);
    setEditorError(null);
    const formData = new FormData(event.currentTarget);
    try {
      const input = modelMappingInputFromForm(formData, t);
      if (editingMapping) {
        const updated = await ModelMappingService.updateMapping(editingMapping.id, input);
        setMappings((current) => current.map((item) => item.id === updated.id ? updated : item));
      } else {
        const created = await ModelMappingService.createMapping(input);
        setMappings((current) => [created, ...current]);
      }
      setIsEditorOpen(false);
      setEditingMapping(null);
      setEditorError(null);
    } catch (error) {
      setEditorError(modelMappingFormErrorsFromError(error, t));
    } finally {
      setSaving(false);
    }
  };

  const handleDeleteMapping = async (mapping: ModelMappingRule) => {
    setLoadError(null);
    try {
      const deleted = await ModelMappingService.deleteMapping(mapping.id);
      if (deleted) {
        setMappings((current) => current.filter((item) => item.id !== mapping.id));
      }
    } catch (error) {
      setLoadError(error instanceof Error ? error.message : t('admin.model.mapping.errors.deleteMapping'));
    }
  };

  const handleRelationMappingUpdated = (updated: ModelMappingRule) => {
    setMappings((current) => current.map((item) => item.id === updated.id ? updated : item));
    setEditingRelationMapping(null);
  };

  return (
    <div className="flex h-full min-h-0 w-full flex-col overflow-hidden bg-slate-50 text-slate-900 dark:bg-[#0f0f10] dark:text-white">
      <AdminTableShell
        data-admin-model-mapping-table-card
        className="flex-1 min-h-0"
        viewportClassName="min-h-0 flex-1"
        viewportProps={{ 'data-admin-model-mapping-table-viewport': true }}
        header={(
          <div data-admin-model-mapping-toolbar className="border-b border-slate-200 bg-white p-3 dark:border-white/10 dark:bg-[#171717]">
            <div className="grid min-w-0 gap-3 xl:grid-cols-[auto_minmax(20rem,1fr)] xl:items-center">
              <div
                data-admin-model-mapping-scope-filter
                className="flex w-full min-w-0 items-center overflow-x-auto rounded-lg border border-slate-200 bg-slate-50 p-0.5 dark:border-white/10 dark:bg-[#121212] xl:w-fit"
              >
                {([
                  { value: 'global', label: t('admin.model.mapping.scope.global') },
                  { value: 'vendor', label: t('admin.model.mapping.scope.vendor') },
                  { value: 'channel', label: t('admin.model.mapping.scope.channel') },
                  { value: 'all', label: t('admin.model.mapping.scope.all') },
                ] as Array<{ value: ModelMappingBindingFilter; label: string }>).map((tab) => (
                  <button
                    key={tab.value}
                    type="button"
                    onClick={() => handleBindingFilterChange(tab.value)}
                    aria-pressed={bindingFilter === tab.value}
                    className={`inline-flex h-8 shrink-0 items-center justify-center whitespace-nowrap rounded-md px-3 text-sm font-medium transition ${
                      bindingFilter === tab.value
                        ? 'bg-white text-slate-950 shadow-sm ring-1 ring-slate-200 dark:bg-white/10 dark:text-white dark:ring-white/10'
                        : 'text-slate-500 hover:text-slate-900 dark:text-slate-400 dark:hover:text-white'
                    }`}
                  >
                    {tab.label}
                  </button>
                ))}
              </div>
              <div className="flex min-w-0 items-center gap-2 xl:justify-end">
                <div data-admin-model-mapping-search className="relative min-w-0 flex-1 xl:max-w-md">
                  <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-slate-400" />
                  <input
                    type="search"
                    value={search}
                    onChange={(event) => setSearch(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter') {
                        void loadMappings();
                      }
                    }}
                    placeholder={t('admin.model.mapping.search.placeholder')}
                    className="h-9 w-full rounded-lg border border-slate-200 bg-white pl-9 pr-3 text-sm text-slate-900 outline-none transition placeholder:text-slate-400 focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500/15 dark:border-white/10 dark:bg-[#121212] dark:text-white"
                  />
                </div>
                <button
                  data-admin-model-mapping-primary-action
                  type="button"
                  onClick={() => openCreateMappingWithBinding(bindingFilter === 'all' ? 'global' : bindingFilter)}
                  className="inline-flex h-9 shrink-0 items-center justify-center gap-2 whitespace-nowrap rounded-lg bg-indigo-600 px-3.5 text-sm font-semibold text-white shadow-sm transition hover:bg-indigo-700 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-indigo-500 focus-visible:ring-offset-2"
                >
                  <Plus className="h-4 w-4" />
                  {t('admin.model.mapping.actions.add')}
                </button>
              </div>
            </div>
            {(loadError || catalogError) && (
              <div className="mt-3 border-l-2 border-rose-500 bg-rose-50 px-3 py-2 text-sm text-rose-700 dark:bg-rose-500/10 dark:text-rose-200">
                {loadError ?? catalogError}
              </div>
            )}
          </div>
        )}
      >
          <table data-admin-model-mapping-table className="w-full min-w-[920px] text-left text-sm">
            <thead className="sticky top-0 z-10 border-b border-slate-200 bg-slate-50 text-xs text-slate-500 dark:border-white/10 dark:bg-[#121212] dark:text-slate-400">
              <tr>
                <th className="px-5 py-3 font-medium">{t('admin.model.mapping.table.scope')}</th>
                <th className="px-5 py-3 font-medium">{t('admin.model.mapping.table.binding')}</th>
                <th className="px-5 py-3 font-medium">{t('admin.model.mapping.table.relations')}</th>
                <th className="px-5 py-3 font-medium">{t('admin.model.mapping.table.status')}</th>
                <th className="px-5 py-3 text-right font-medium">{t('admin.model.table.actions')}</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-100 dark:divide-white/10">
              {loading ? (
                <BusinessStateTableRow colSpan={5} icon={<Loader2 className="h-5 w-5 animate-spin" />} title={t('admin.model.mapping.state.loading')} />
              ) : filteredMappings.length === 0 ? (
                <BusinessStateTableRow colSpan={5} icon={<ArrowRightLeft className="h-5 w-5" />} title={t('admin.model.mapping.state.empty')} />
              ) : filteredMappings.map((mapping) => (
                <tr key={mapping.id} className="transition hover:bg-slate-50 dark:hover:bg-white/5">
                  <td className="px-5 py-3.5">
                    <div className="font-semibold text-slate-900 dark:text-white">{t(`admin.model.mapping.scope.${mapping.bindingType}`)}</div>
                    <div className="mt-1 text-xs text-slate-500">{mappingBindingIdentity(mapping, t)}</div>
                  </td>
                  <td className="px-5 py-3.5">
                    <ModelMappingBindingsCell mapping={mapping} />
                  </td>
                  <td className="px-5 py-3.5">
                    <ModelMappingRelationsCell mapping={mapping} onOpenEditor={openRelationEditor} />
                  </td>
                  <td className="px-5 py-3.5"><StatusPill value={mapping.enabled ? 'active' : 'disabled'} /></td>
                  <td className="px-5 py-3.5">
                    <div className="flex justify-end gap-1">
                      <button type="button" onClick={() => openEditMapping(mapping)} aria-label={t('common.actions.edit')} title={t('common.actions.edit')} className="inline-flex h-8 w-8 items-center justify-center rounded-md text-slate-500 transition hover:bg-slate-100 hover:text-slate-900 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-indigo-500 dark:hover:bg-white/10 dark:hover:text-white">
                        <Edit className="h-4 w-4" />
                      </button>
                      <button type="button" onClick={() => void handleDeleteMapping(mapping)} aria-label={t('common.actions.delete')} title={t('common.actions.delete')} className="inline-flex h-8 w-8 items-center justify-center rounded-md text-slate-400 transition hover:bg-rose-50 hover:text-rose-600 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-rose-500 dark:hover:bg-rose-500/10 dark:hover:text-rose-300">
                        <Trash2 className="h-4 w-4" />
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
      </AdminTableShell>

      {isEditorOpen && (
        <ModelMappingFormModal
          mapping={editingMapping}
          vendors={vendors}
          saving={saving}
          error={editorError}
          defaultBindingType={bindingFilter === 'all' ? 'global' : bindingFilter}
          onClearFieldError={clearEditorFieldError}
          onClearRowError={clearEditorRowError}
          onSubmit={handleSaveMapping}
          onClose={closeEditor}
        />
      )}
      {editingRelationMapping && (
        <ModelMappingRelationEditorModal
          mapping={editingRelationMapping}
          saving={saving}
          error={editorError}
          onClearRowError={clearEditorRowError}
          onUpdated={handleRelationMappingUpdated}
          onError={setEditorError}
          onSavingChange={setSaving}
          onClose={() => {
            if (saving) {
              return;
            }
            setEditingRelationMapping(null);
            setEditorError(null);
          }}
        />
      )}
    </div>
  );
}

function ModelMappingFormModal({
  mapping,
  vendors,
  saving,
  error,
  defaultBindingType,
  onClearFieldError,
  onClearRowError,
  onSubmit,
  onClose,
}: {
  mapping: ModelMappingRule | null;
  vendors: readonly Vendor[];
  saving: boolean;
  error: ModelMappingFormErrors | null;
  defaultBindingType: ModelMappingRule['bindingType'];
  onClearFieldError: (field: ModelMappingFieldErrorKey) => void;
  onClearRowError: (rowId: string, field: ModelMappingRowFieldKey) => void;
  onSubmit: (event: React.FormEvent<HTMLFormElement>) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [bindingType, setBindingType] = useState<ModelMappingRule['bindingType']>(mapping?.bindingType ?? defaultBindingType);
  const [activeVendorPicker, setActiveVendorPicker] = useState<'source' | 'target' | null>(null);
  const [sourceVendorCode, setSourceVendorCode] = useState<string>(mapping?.sourceVendorCode ?? '');
  const [targetVendorCode, setTargetVendorCode] = useState<string>(mapping?.targetVendorCode ?? '');
  const [mappingBindings, setMappingBindings] = useState<ModelMappingBindingDraft[]>(() => createMappingBindingDrafts(mapping, defaultBindingType));
  const [mappingRows, setMappingRows] = useState<ModelMappingRowDraft[]>(() => createMappingRowDrafts(mapping));
  const sourceVendor = vendors.find((vendor) => vendor.vendorCode === sourceVendorCode) ?? null;
  const targetVendor = vendors.find((vendor) => vendor.vendorCode === targetVendorCode) ?? null;
  const fieldErrors = error?.fieldErrors ?? {};
  const rowErrors = error?.rowErrors ?? {};
  const firstErrorKey = error?.firstErrorKey ?? null;

  const syncBindingFields = (nextBinding: ModelMappingRule['bindingType']) => {
    setBindingType(nextBinding);
    clearFieldError('mappingBindings');
    setMappingBindings((current) => {
      const [first, ...rest] = current;
      const nextFirst = normalizeBindingDraftForType({
        ...(first ?? createMappingBindingDraft(null, nextBinding)),
        bindingType: nextBinding,
      }, sourceVendorCode);
      return [nextFirst, ...rest];
    });
  };

  const clearFieldError = (field: ModelMappingFieldErrorKey) => {
    onClearFieldError(field);
  };

  const clearRowError = (rowId: string, field: ModelMappingRowFieldKey) => {
    onClearRowError(rowId, field);
  };

  useEffect(() => {
    if (!firstErrorKey) {
      return;
    }
    const escapedKey = typeof CSS !== 'undefined' && typeof CSS.escape === 'function'
      ? CSS.escape(firstErrorKey)
      : firstErrorKey.replace(/["\\]/gu, '\\$&');
    const target = document.querySelector<HTMLElement>(`[data-model-mapping-error-key="${escapedKey}"]`);
    target?.scrollIntoView({ block: 'center', behavior: 'smooth' });
    target?.focus?.();
  }, [firstErrorKey]);

  const handleVendorSelect = (kind: 'source' | 'target', vendor: Vendor) => {
    if (kind === 'source') {
      setSourceVendorCode(vendor.vendorCode);
      clearFieldError('sourceVendorCode');
      setMappingBindings((current) => current.map((binding) => (
        binding.bindingType === 'vendor' && !binding.bindingCode.trim()
          ? { ...binding, bindingCode: vendor.vendorCode, bindingName: vendor.name }
          : binding
      )));
      return;
    }
    setTargetVendorCode(vendor.vendorCode);
    clearFieldError('targetVendorCode');
  };

  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-slate-950/50 p-4 backdrop-blur-sm">
      <div className="flex h-[90vh] w-full max-w-[84rem] flex-col overflow-hidden rounded-3xl border border-slate-200 bg-white shadow-2xl dark:border-white/10 dark:bg-[#171719]">
        <div className="shrink-0 flex items-center justify-between border-b border-slate-200 px-5 py-4 dark:border-white/10">
          <div>
            <h3 className="text-lg font-semibold text-slate-900 dark:text-white">{mapping ? t('admin.model.mapping.form.editTitle') : t('admin.model.mapping.form.createTitle')}</h3>
            <p className="mt-1 text-sm text-slate-500 dark:text-slate-400">{t('admin.model.mapping.form.helper')}</p>
          </div>
          <button type="button" onClick={onClose} disabled={saving} className="text-slate-400 hover:text-slate-600 disabled:opacity-50">
            <X className="h-5 w-5" />
          </button>
        </div>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            const form = event.currentTarget;
            writeHiddenFormValue(form, 'bindingType', bindingType);
            writeHiddenFormValue(form, 'sourceVendorCode', sourceVendorCode);
            writeHiddenFormValue(form, 'targetVendorCode', targetVendorCode);
            writeHiddenFormValue(form, 'rowsJson', JSON.stringify(mappingRows));
            writeHiddenFormValue(form, 'bindingsJson', JSON.stringify(mappingBindings));
            onSubmit(event);
          }}
          className="flex min-h-0 flex-1 flex-col"
        >
          <div data-model-mapping-form-scroll className="min-h-0 flex-1 overflow-y-auto p-5">
            <input name="bindingType" type="hidden" value={bindingType} />
            <input name="sourceVendorCode" type="hidden" value={sourceVendorCode} />
            <input name="targetVendorCode" type="hidden" value={targetVendorCode} />
            <input name="rowsJson" type="hidden" value={JSON.stringify(mappingRows)} />
            <input name="bindingsJson" type="hidden" value={JSON.stringify(mappingBindings)} />
            {error?.message && (
              <div className="mb-5 rounded-xl border border-rose-200 bg-rose-50 px-4 py-3 text-sm font-medium text-rose-700 dark:border-rose-500/30 dark:bg-rose-500/10 dark:text-rose-200">
                {error.message}
              </div>
            )}
            <div className="grid min-h-0 flex-1 gap-5 lg:grid-cols-[320px_minmax(0,1fr)]">
            <section className="rounded-2xl border border-slate-200 bg-slate-50 p-4 dark:border-white/10 dark:bg-white/5">
              <div className="mb-4 flex items-center justify-between">
                <div>
                  <h4 className="text-sm font-semibold text-slate-900 dark:text-white">{t('admin.model.mapping.form.scopeTitle')}</h4>
                  <p className="mt-1 text-xs text-slate-500 dark:text-slate-400">{t('admin.model.mapping.form.scopeHint')}</p>
                </div>
                <span className="rounded-full bg-white px-3 py-1 text-xs font-semibold text-slate-600 ring-1 ring-slate-200 dark:bg-[#171719] dark:text-slate-300 dark:ring-white/10">{t(`admin.model.mapping.scope.${bindingType}`)}</span>
              </div>
              <label className="block">
                <span className="mb-1.5 block text-sm font-medium text-slate-700 dark:text-slate-300">{t('admin.model.mapping.form.scope')}</span>
                <select
                  value={bindingType}
                  onChange={(event) => syncBindingFields(event.target.value as ModelMappingRule['bindingType'])}
                  className="w-full rounded-xl border border-slate-200 bg-white px-3 py-2 text-sm outline-none focus:border-indigo-500 focus:ring-1 focus:ring-indigo-500 dark:border-white/10 dark:bg-[#171719] dark:text-white"
                >
                  <option value="global">{t('admin.model.mapping.scope.global')}</option>
                  <option value="vendor">{t('admin.model.mapping.scope.vendor')}</option>
                  <option value="channel_group">{t('admin.model.mapping.scope.channelGroup')}</option>
                  <option value="channel">{t('admin.model.mapping.scope.channel')}</option>
                  <option value="provider_account">{t('admin.model.mapping.scope.providerAccount')}</option>
                  <option value="site">{t('admin.model.mapping.scope.site')}</option>
                  <option value="site_service">{t('admin.model.mapping.scope.siteService')}</option>
                </select>
              </label>
              <div className="mt-4 grid gap-3">
                <button
                  type="button"
                  onClick={() => setActiveVendorPicker('source')}
                  aria-invalid={Boolean(fieldErrors.sourceVendorCode)}
                  data-model-mapping-error-key="sourceVendorCode"
                  className={`rounded-xl border bg-white px-3 py-2 text-left text-sm text-slate-700 transition dark:bg-[#171719] dark:text-slate-200 ${fieldErrors.sourceVendorCode ? 'border-rose-300 hover:border-rose-400 hover:bg-rose-50 dark:border-rose-500/50 dark:hover:bg-rose-500/10' : 'border-slate-200 hover:border-indigo-300 hover:bg-indigo-50 dark:border-white/10 dark:hover:border-indigo-500/50 dark:hover:bg-indigo-500/10'}`}
                >
                  <span className="block text-xs font-medium uppercase tracking-wide text-slate-400">{t('admin.model.mapping.form.sourceVendor')}</span>
                  <span className="mt-1 block truncate font-semibold">{sourceVendor?.name ?? (sourceVendorCode || t('admin.model.mapping.form.selectVendor'))}</span>
                  <span className="mt-1 block truncate text-xs text-slate-500">{sourceVendor?.vendorCode ?? (sourceVendorCode || t('admin.model.mapping.noData'))}</span>
                </button>
                {fieldErrors.sourceVendorCode && <span className="-mt-2 block text-xs font-medium text-rose-600 dark:text-rose-300">{fieldErrors.sourceVendorCode}</span>}
                <button
                  type="button"
                  onClick={() => setActiveVendorPicker('target')}
                  aria-invalid={Boolean(fieldErrors.targetVendorCode)}
                  data-model-mapping-error-key="targetVendorCode"
                  className={`rounded-xl border bg-white px-3 py-2 text-left text-sm text-slate-700 transition dark:bg-[#171719] dark:text-slate-200 ${fieldErrors.targetVendorCode ? 'border-rose-300 hover:border-rose-400 hover:bg-rose-50 dark:border-rose-500/50 dark:hover:bg-rose-500/10' : 'border-slate-200 hover:border-indigo-300 hover:bg-indigo-50 dark:border-white/10 dark:hover:border-indigo-500/10'}`}
                >
                  <span className="block text-xs font-medium uppercase tracking-wide text-slate-400">{t('admin.model.mapping.form.targetVendor')}</span>
                  <span className="mt-1 block truncate font-semibold">{targetVendor?.name ?? (targetVendorCode || t('admin.model.mapping.form.selectVendor'))}</span>
                  <span className="mt-1 block truncate text-xs text-slate-500">{targetVendor?.vendorCode ?? (targetVendorCode || t('admin.model.mapping.noData'))}</span>
                </button>
                {fieldErrors.targetVendorCode && <span className="-mt-2 block text-xs font-medium text-rose-600 dark:text-rose-300">{fieldErrors.targetVendorCode}</span>}
              </div>
              <div className="mt-5 border-t border-slate-200 pt-4 dark:border-white/10">
                <div className="mb-3 flex items-center justify-between gap-3">
                  <h4 className="text-sm font-semibold text-slate-900 dark:text-white">{t('admin.model.mapping.form.bindingTitle')}</h4>
                  <button
                    type="button"
                    onClick={() => {
                      clearFieldError('mappingBindings');
                      setMappingBindings((current) => [...current, createMappingBindingDraft(null, bindingType)]);
                    }}
                    className="inline-flex items-center gap-1.5 rounded-lg border border-slate-200 bg-white px-2.5 py-1.5 text-xs font-medium text-slate-600 transition hover:bg-slate-50 dark:border-white/10 dark:bg-[#171719] dark:text-slate-300 dark:hover:bg-white/10"
                  >
                    <Plus className="h-3.5 w-3.5" />
                    {t('admin.model.mapping.form.addBinding')}
                  </button>
                </div>
                {fieldErrors.mappingBindings && (
                  <div
                    tabIndex={-1}
                    data-model-mapping-error-key="mappingBindings"
                    className="mb-3 rounded-lg border border-rose-200 bg-rose-50 px-3 py-2 text-xs font-medium text-rose-700 outline-none dark:border-rose-500/30 dark:bg-rose-500/10 dark:text-rose-200"
                  >
                    {fieldErrors.mappingBindings}
                  </div>
                )}
                <div className="space-y-3">
                  {mappingBindings.map((binding) => (
                    <div key={binding.id} className="rounded-xl border border-slate-200 bg-white p-3 dark:border-white/10 dark:bg-[#171719]">
                      <div className="flex items-center gap-2">
                        <select
                          value={binding.bindingType}
                          onChange={(event) => {
                            clearFieldError('mappingBindings');
                            const nextType = event.target.value as ModelMappingRule['bindingType'];
                            setMappingBindings((current) => current.map((item) => item.id === binding.id
                              ? normalizeBindingDraftForType({ ...item, bindingType: nextType }, sourceVendorCode)
                              : item));
                          }}
                          className="min-w-0 flex-1 rounded-lg border border-slate-200 bg-white px-2 py-2 text-xs outline-none focus:border-indigo-500 focus:ring-1 focus:ring-indigo-500 dark:border-white/10 dark:bg-[#121214] dark:text-white"
                        >
                          <option value="global">{t('admin.model.mapping.scope.global')}</option>
                          <option value="vendor">{t('admin.model.mapping.scope.vendor')}</option>
                          <option value="channel_group">{t('admin.model.mapping.scope.channelGroup')}</option>
                          <option value="channel">{t('admin.model.mapping.scope.channel')}</option>
                          <option value="provider_account">{t('admin.model.mapping.scope.providerAccount')}</option>
                          <option value="site">{t('admin.model.mapping.scope.site')}</option>
                          <option value="site_service">{t('admin.model.mapping.scope.siteService')}</option>
                        </select>
                        <button
                          type="button"
                          onClick={() => {
                            clearFieldError('mappingBindings');
                            setMappingBindings((current) => current.length > 1 ? current.filter((item) => item.id !== binding.id) : current);
                          }}
                          className="inline-flex h-9 w-9 items-center justify-center rounded-lg text-rose-500 transition hover:bg-rose-50 dark:hover:bg-rose-500/10"
                          title={t('admin.model.mapping.form.removeBinding')}
                        >
                          <Trash2 className="h-4 w-4" />
                        </button>
                      </div>
                      {binding.bindingType !== 'global' && (
                        <input
                          value={binding.bindingCode}
                          onChange={(event) => {
                            clearFieldError('mappingBindings');
                            setMappingBindings((current) => current.map((item) => item.id === binding.id ? { ...item, bindingCode: event.target.value, bindingName: event.target.value } : item));
                          }}
                          placeholder={t('admin.model.mapping.form.bindingCode')}
                          aria-invalid={Boolean(fieldErrors.mappingBindings)}
                          className={`mt-2 w-full rounded-lg border bg-white px-2 py-2 text-xs font-mono outline-none focus:ring-1 dark:bg-[#121214] dark:text-white ${fieldErrors.mappingBindings ? 'border-rose-300 focus:border-rose-500 focus:ring-rose-500 dark:border-rose-500/50' : 'border-slate-200 focus:border-indigo-500 focus:ring-indigo-500 dark:border-white/10'}`}
                        />
                      )}
                    </div>
                  ))}
                </div>
              </div>
            </section>
            <section className="rounded-2xl border border-slate-200 bg-white p-4 dark:border-white/10 dark:bg-[#121214]">
              <div className="mb-4 flex items-center justify-between gap-3">
                <div>
                  <h4 className="text-sm font-semibold text-slate-900 dark:text-white">{t('admin.model.mapping.form.mappingRowsTitle')}</h4>
                </div>
                <button
                  type="button"
                  onClick={() => setMappingRows((current) => [...current, createMappingRowDraft(null)])}
                  className="inline-flex items-center gap-1.5 rounded-lg border border-slate-200 bg-white px-3 py-1.5 text-xs font-medium text-slate-600 transition hover:bg-slate-50 dark:border-white/10 dark:bg-[#171719] dark:text-slate-300 dark:hover:bg-white/10"
                >
                  <Plus className="h-3.5 w-3.5" />
                  {t('admin.model.mapping.form.addRow')}
                </button>
              </div>
              <ModelMappingRowsTable
                rows={mappingRows}
                sourceVendorCode={sourceVendorCode}
                targetVendorCode={targetVendorCode}
                searchPlaceholder={t('admin.model.mapping.form.modelPicker.searchPlaceholder')}
                inputPlaceholder={t('admin.model.mapping.form.modelInputPlaceholder')}
                fieldErrors={fieldErrors}
                rowErrors={rowErrors}
                onClearRowError={clearRowError}
                onChange={setMappingRows}
              />
            </section>
            </div>
          </div>
          <div data-model-mapping-form-footer className="shrink-0 border-t border-slate-200 px-5 py-4 dark:border-white/10">
            <div className="flex items-center justify-between gap-3">
              <div className="text-xs text-slate-500 dark:text-slate-400">{t('admin.model.mapping.form.saveHint')}</div>
              <div className="flex items-center gap-3">
                <button type="button" onClick={onClose} disabled={saving} className="rounded-xl border border-slate-200 px-4 py-2 text-sm font-medium text-slate-700 disabled:opacity-50 dark:border-white/10 dark:text-slate-200">
                  {t('common.actions.cancel')}
                </button>
                <button type="submit" disabled={saving} className="inline-flex items-center gap-2 rounded-xl bg-indigo-600 px-4 py-2 text-sm font-semibold text-white hover:bg-indigo-700 disabled:opacity-60">
                  {saving && <Loader2 className="h-4 w-4 animate-spin" />}
                  {t('common.actions.save')}
                </button>
              </div>
            </div>
          </div>
        </form>
        {activeVendorPicker && (
          <VendorPickerModal
            vendors={vendors}
            title={activeVendorPicker === 'source' ? t('admin.model.mapping.form.sourceVendor') : t('admin.model.mapping.form.targetVendor')}
            searchPlaceholder={t('admin.model.mapping.form.vendorPicker.searchPlaceholder')}
            onSelect={(vendor) => {
              handleVendorSelect(activeVendorPicker, vendor);
              setActiveVendorPicker(null);
            }}
            onClose={() => setActiveVendorPicker(null)}
          />
        )}
      </div>
    </div>
  );
}


function ModelMappingRowsTable({
  rows,
  sourceVendorCode,
  targetVendorCode,
  searchPlaceholder,
  inputPlaceholder,
  fieldErrors,
  rowErrors,
  onClearRowError,
  onChange,
}: {
  rows: readonly ModelMappingRowDraft[];
  sourceVendorCode: string;
  targetVendorCode: string;
  searchPlaceholder: string;
  inputPlaceholder: string;
  fieldErrors: Partial<Record<ModelMappingFieldErrorKey, string>>;
  rowErrors: Record<string, ModelMappingRowErrors>;
  onClearRowError: (rowId: string, field: ModelMappingRowFieldKey) => void;
  onChange: React.Dispatch<React.SetStateAction<ModelMappingRowDraft[]>>;
}) {
  const { t } = useTranslation();

  const updateRow = (rowId: string, field: ModelMappingRowFieldKey, value: string) => {
    onClearRowError(rowId, field);
    onChange((current) => current.map((row) => row.id === rowId ? { ...row, [field]: value } : row));
  };

  return (
    <div className="overflow-visible rounded-2xl border border-slate-200 dark:border-white/10">
      {fieldErrors.mappingRows && (
        <div
          tabIndex={-1}
          data-model-mapping-error-key="mappingRows"
          className="border-b border-rose-200 bg-rose-50 px-3 py-2 text-xs font-medium text-rose-700 outline-none dark:border-rose-500/30 dark:bg-rose-500/10 dark:text-rose-200"
        >
          {fieldErrors.mappingRows}
        </div>
      )}
      <table className="w-full table-fixed text-left text-sm">
        <thead className="bg-slate-50 text-xs font-semibold uppercase tracking-wide text-slate-500 dark:bg-white/5 dark:text-slate-400">
          <tr>
            <th className="w-1/2 px-3 py-2">{t('admin.model.mapping.form.sourceModel')}</th>
            <th className="w-1/2 px-3 py-2">{t('admin.model.mapping.form.targetModel')}</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-slate-100 dark:divide-white/10">
          {rows.map((row) => (
            <tr key={row.id}>
              <td className="px-3 py-2 align-top">
                <ModelComboboxCell
                  value={row.sourceModel}
                  vendorCode={sourceVendorCode}
                  searchPlaceholder={searchPlaceholder}
                  inputPlaceholder={inputPlaceholder}
                  errorMessage={rowErrors[row.id]?.sourceModel}
                  errorKey={`${row.id}.sourceModel`}
                  onChange={(value) => updateRow(row.id, 'sourceModel', value)}
                />
              </td>
              <td className="px-3 py-2 align-top">
                <ModelComboboxCell
                  value={row.targetModel}
                  vendorCode={targetVendorCode}
                  searchPlaceholder={searchPlaceholder}
                  inputPlaceholder={inputPlaceholder}
                  errorMessage={rowErrors[row.id]?.targetModel}
                  errorKey={`${row.id}.targetModel`}
                  onChange={(value) => updateRow(row.id, 'targetModel', value)}
                />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ModelComboboxCell({
  value,
  vendorCode,
  searchPlaceholder,
  inputPlaceholder,
  errorMessage,
  errorKey,
  onChange,
}: {
  value: string;
  vendorCode: string;
  searchPlaceholder: string;
  inputPlaceholder: string;
  errorMessage?: string;
  errorKey: string;
  onChange: (value: string) => void;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState('');
  const [models, setModels] = useState<ModelMappingModelOption[]>([]);
  const [loading, setLoading] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const requestSequenceRef = useRef(0);

  useEffect(() => {
    if (!open) {
      return undefined;
    }
    const requestSequence = requestSequenceRef.current + 1;
    requestSequenceRef.current = requestSequence;
    const timeout = window.setTimeout(() => {
      setLoading(true);
      setLoadError(null);
      void ModelMappingService.fetchModelOptionsPage({
        vendorCode: vendorCode.trim() || undefined,
        q: search.trim() || undefined,
        page: 1,
        pageSize: 50,
      }).then((page) => {
        if (requestSequenceRef.current === requestSequence) {
          setModels(page.items);
        }
      }).catch((error: unknown) => {
        if (requestSequenceRef.current === requestSequence) {
          setModels([]);
          setLoadError(error instanceof Error ? error.message : t('admin.model.mapping.errors.loadOptions'));
        }
      }).finally(() => {
        if (requestSequenceRef.current === requestSequence) {
          setLoading(false);
        }
      });
    }, 200);
    return () => {
      window.clearTimeout(timeout);
      requestSequenceRef.current += 1;
    };
  }, [open, search, vendorCode]);

  useEffect(() => {
    if (!open) {
      setSearch('');
      return undefined;
    }
    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) {
        return;
      }
      if (rootRef.current?.contains(target)) {
        return;
      }
      setOpen(false);
    };
    document.addEventListener('pointerdown', handlePointerDown);
    return () => document.removeEventListener('pointerdown', handlePointerDown);
  }, [open]);

  return (
    <div ref={rootRef} className="relative">
      <input
        value={value}
        onFocus={() => setOpen(true)}
        onChange={(event) => {
          onChange(event.target.value);
          setSearch(event.target.value);
          setOpen(true);
        }}
        placeholder={inputPlaceholder}
        aria-invalid={Boolean(errorMessage)}
        data-model-mapping-error-key={errorKey}
        className={`w-full rounded-xl border bg-white px-3 py-2 pr-9 text-sm font-mono text-slate-900 outline-none transition focus:ring-1 dark:bg-[#171719] dark:text-white ${errorMessage ? 'border-rose-300 focus:border-rose-500 focus:ring-rose-500 dark:border-rose-500/50' : 'border-slate-200 focus:border-indigo-500 focus:ring-indigo-500 dark:border-white/10'}`}
      />
      <button
        type="button"
        onClick={() => setOpen((current) => !current)}
        className="absolute right-2 top-1/2 -translate-y-1/2 rounded-md p-1 text-slate-400 transition hover:bg-slate-100 hover:text-slate-600 dark:hover:bg-white/10 dark:hover:text-slate-200"
        aria-label={searchPlaceholder}
      >
        <ChevronDown className="h-4 w-4" />
      </button>
      {open && (
        <div className="absolute left-0 right-0 z-[75] mt-2 rounded-2xl border border-slate-200 bg-white p-2 shadow-2xl dark:border-white/10 dark:bg-[#171719]">
          <div className="max-h-64 space-y-1 overflow-y-auto">
            {loading ? (
              <div className="px-3 py-4 text-sm text-slate-500">{t('admin.model.mapping.form.loadingCatalog')}</div>
            ) : loadError ? (
              <div className="px-3 py-4 text-sm text-rose-600 dark:text-rose-300">{loadError}</div>
            ) : models.length === 0 ? (
              <div className="px-3 py-4 text-sm text-slate-500">{t('admin.model.mapping.form.noModels')}</div>
            ) : models.map((model) => {
              const checked = model.model === value;
              const optionClassName = [
                'flex w-full items-center justify-between gap-3 rounded-xl px-3 py-2 text-left text-sm transition',
                checked
                  ? 'bg-indigo-50 text-indigo-700 dark:bg-indigo-500/10 dark:text-indigo-200'
                  : 'text-slate-700 hover:bg-slate-50 dark:text-slate-200 dark:hover:bg-white/10',
              ].join(' ');
              return (
                <button
                  key={model.id}
                  type="button"
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={() => {
                    onChange(model.model);
                    setOpen(false);
                    setSearch('');
                  }}
                  className={optionClassName}
                >
                  <span className="min-w-0">
                    <span className="block truncate font-medium">{model.displayName}</span>
                    <span className="block truncate font-mono text-xs text-slate-500">{model.model}</span>
                  </span>
                  {checked && <Check className="h-4 w-4 text-indigo-600 dark:text-indigo-300" />}
                </button>
              );
            })}
          </div>
        </div>
      )}
      {errorMessage && <span className="mt-1.5 block text-xs font-medium text-rose-600 dark:text-rose-300">{errorMessage}</span>}
    </div>
  );
}

function ModelMappingRelationsCell({
  mapping,
  onOpenEditor,
}: {
  mapping: ModelMappingRule;
  onOpenEditor: (mapping: ModelMappingRule) => void;
}) {
  const { t } = useTranslation();
  const visibleItems = mapping.mappingItems.slice(0, 3);
  return (
    <button
      type="button"
      onClick={() => onOpenEditor(mapping)}
      className="block w-full rounded-xl border border-transparent px-3 py-2 text-left transition hover:border-indigo-200 hover:bg-indigo-50 dark:hover:border-indigo-500/30 dark:hover:bg-indigo-500/10"
    >
      <div className="space-y-1.5">
        {visibleItems.length === 0 ? (
          <span className="text-xs text-slate-400">{t('admin.model.mapping.noData')}</span>
        ) : visibleItems.map((item) => (
          <div key={item.id} className="flex min-w-0 items-center gap-2 text-xs">
            <span className="truncate font-mono font-semibold text-slate-800 dark:text-slate-100">{item.sourceModel}</span>
            <ArrowRightLeft className="h-3.5 w-3.5 shrink-0 text-slate-400" />
            <span className="truncate font-mono text-slate-600 dark:text-slate-300">{item.targetModel}</span>
          </div>
        ))}
      </div>
      {mapping.mappingItems.length > visibleItems.length && (
        <div className="mt-1.5 text-xs font-medium text-indigo-600 dark:text-indigo-300">+{mapping.mappingItems.length - visibleItems.length}</div>
      )}
    </button>
  );
}

function ModelMappingRelationEditorModal({
  mapping,
  saving,
  error,
  onClearRowError,
  onUpdated,
  onError,
  onSavingChange,
  onClose,
}: {
  mapping: ModelMappingRule;
  saving: boolean;
  error: ModelMappingFormErrors | null;
  onClearRowError: (rowId: string, field: ModelMappingRowFieldKey) => void;
  onUpdated: (mapping: ModelMappingRule) => void;
  onError: (error: ModelMappingFormErrors | null) => void;
  onSavingChange: (saving: boolean) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [mappingRows, setMappingRows] = useState<ModelMappingRowDraft[]>(() => createMappingRowDrafts(mapping));
  const fieldErrors = error?.fieldErrors ?? {};
  const rowErrors = error?.rowErrors ?? {};
  const firstErrorKey = error?.firstErrorKey ?? null;

  useEffect(() => {
    if (!firstErrorKey) {
      return;
    }
    const escapedKey = typeof CSS !== 'undefined' && typeof CSS.escape === 'function'
      ? CSS.escape(firstErrorKey)
      : firstErrorKey.replace(/["\\]/gu, '\\$&');
    const target = document.querySelector<HTMLElement>(`[data-model-mapping-error-key="${escapedKey}"]`);
    target?.scrollIntoView({ block: 'center', behavior: 'smooth' });
    target?.focus?.();
  }, [firstErrorKey]);

  const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    onSavingChange(true);
    onError(null);
    const form = event.currentTarget;
    writeHiddenFormValue(form, 'rowsJson', JSON.stringify(mappingRows));
    const formData = new FormData(form);
    try {
      const errors = createEmptyModelMappingFormErrors();
      const rows = readMappingRowsFromForm(formData, errors, t);
      validateUniqueModelMappingRows(rows, errors, t);
      throwModelMappingValidationErrorIfNeeded(errors, t);
      const input: ModelMappingUpdateInput = {
        mappingItems: rows.map((row): ModelMappingRuleItemInput => ({
          id: persistedChildId(row.persistedId),
          sourceModel: row.sourceModel,
          targetModel: row.targetModel,
          enabled: row.enabled,
        })),
      };
      const updated = await ModelMappingService.updateMapping(mapping.id, input);
      onUpdated(updated);
    } catch (caught) {
      onError(modelMappingFormErrorsFromError(caught, t));
    } finally {
      onSavingChange(false);
    }
  };

  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-slate-950/50 p-4 backdrop-blur-sm">
      <div className="flex h-[90vh] w-full max-w-5xl flex-col overflow-hidden rounded-3xl border border-slate-200 bg-white shadow-2xl dark:border-white/10 dark:bg-[#171719]">
        <div className="shrink-0 flex items-center justify-between border-b border-slate-200 px-5 py-4 dark:border-white/10">
          <div>
            <h3 className="text-lg font-semibold text-slate-900 dark:text-white">{t('admin.model.mapping.relations.editTitle')}</h3>
          </div>
          <button type="button" onClick={onClose} disabled={saving} className="text-slate-400 hover:text-slate-600 disabled:opacity-50">
            <X className="h-5 w-5" />
          </button>
        </div>
        <form onSubmit={handleSubmit} className="flex min-h-0 flex-1 flex-col">
          <div data-model-mapping-relation-form-scroll className="min-h-0 flex-1 overflow-y-auto p-5">
            <input name="rowsJson" type="hidden" value={JSON.stringify(mappingRows)} />
            {error?.message && (
              <div className="mb-4 rounded-xl border border-rose-200 bg-rose-50 px-4 py-3 text-sm font-medium text-rose-700 dark:border-rose-500/30 dark:bg-rose-500/10 dark:text-rose-200">
                {error.message}
              </div>
            )}
            <div className="mb-4 flex items-center justify-end">
              <button
                type="button"
                onClick={() => setMappingRows((current) => [...current, createMappingRowDraft(null)])}
                className="inline-flex items-center gap-1.5 rounded-lg border border-slate-200 bg-white px-3 py-1.5 text-xs font-medium text-slate-600 transition hover:bg-slate-50 dark:border-white/10 dark:bg-[#171719] dark:text-slate-300 dark:hover:bg-white/10"
              >
                <Plus className="h-3.5 w-3.5" />
                {t('admin.model.mapping.form.addRow')}
              </button>
            </div>
            <ModelMappingRowsTable
              rows={mappingRows}
              sourceVendorCode={mapping.sourceVendorCode ?? ''}
              targetVendorCode={mapping.targetVendorCode ?? ''}
              searchPlaceholder={t('admin.model.mapping.form.modelPicker.searchPlaceholder')}
              inputPlaceholder={t('admin.model.mapping.form.modelInputPlaceholder')}
              fieldErrors={fieldErrors}
              rowErrors={rowErrors}
              onClearRowError={onClearRowError}
              onChange={setMappingRows}
            />
          </div>
          <div data-model-mapping-relation-form-footer className="shrink-0 border-t border-slate-200 px-5 py-4 dark:border-white/10">
            <div className="flex items-center justify-end gap-3">
              <button type="button" onClick={onClose} disabled={saving} className="rounded-xl border border-slate-200 px-4 py-2 text-sm font-medium text-slate-700 disabled:opacity-50 dark:border-white/10 dark:text-slate-200">
                {t('common.actions.cancel')}
              </button>
              <button type="submit" disabled={saving} className="inline-flex items-center gap-2 rounded-xl bg-indigo-600 px-4 py-2 text-sm font-semibold text-white hover:bg-indigo-700 disabled:opacity-60">
                {saving && <Loader2 className="h-4 w-4 animate-spin" />}
                {t('common.actions.save')}
              </button>
            </div>
          </div>
        </form>
      </div>
    </div>
  );
}

function ModelMappingBindingsCell({ mapping }: { mapping: ModelMappingRule }) {
  const { t } = useTranslation();
  if (mapping.bindings.length === 0) {
    return <span className="text-xs text-slate-400">{t('admin.model.mapping.noData')}</span>;
  }
  return (
    <div className="flex max-w-[320px] flex-wrap gap-1.5">
      {mapping.bindings.slice(0, 3).map((binding) => (
        <span key={binding.id} className="max-w-[220px] truncate rounded-md bg-slate-100 px-2 py-1 text-xs font-semibold text-slate-600 dark:bg-white/10 dark:text-slate-300">
          {t(`admin.model.mapping.scope.${binding.bindingType}`)}: {binding.bindingType === 'global' ? t('admin.model.mapping.allRequests') : (binding.bindingName || binding.bindingCode || binding.bindingId || t('admin.model.mapping.noData'))}
        </span>
      ))}
      {mapping.bindings.length > 3 && (
        <span className="rounded-md bg-slate-100 px-2 py-1 text-xs font-semibold text-slate-500 dark:bg-white/10 dark:text-slate-400">
          +{mapping.bindings.length - 3}
        </span>
      )}
    </div>
  );
}

function FormInput({
  name,
  label,
  defaultValue,
  required = false,
  error,
  onChange,
}: {
  name: string;
  label: string;
  defaultValue?: string;
  required?: boolean;
  error?: string;
  onChange?: () => void;
}) {
  return (
    <label className="block">
      <span className="mb-1.5 block text-sm font-medium text-slate-700 dark:text-slate-300">{label}</span>
      <input
        name={name}
        defaultValue={defaultValue ?? ''}
        required={required}
        onChange={onChange}
        aria-invalid={Boolean(error)}
        className={`w-full rounded-xl border bg-white px-3 py-2 text-sm text-slate-900 outline-none transition focus:ring-1 dark:bg-white/5 dark:text-white ${error ? 'border-rose-300 focus:border-rose-500 focus:ring-rose-500 dark:border-rose-500/50' : 'border-slate-200 focus:border-indigo-500 focus:ring-indigo-500 dark:border-white/10'}`}
      />
      {error && <span className="mt-1.5 block text-xs font-medium text-rose-600 dark:text-rose-300">{error}</span>}
    </label>
  );
}

const STATUS_PILL_LABEL_KEYS: Record<string, string> = {
  active: 'admin.model.mapping.status.active',
  disabled: 'admin.model.mapping.status.disabled',
};

function StatusPill({ value }: { value: string }) {
  const { t } = useTranslation();
  const labelKey = STATUS_PILL_LABEL_KEYS[value];
  const tone = value === 'active' || value === 'healthy' || value === 'success'
    ? 'bg-emerald-50 text-emerald-700 ring-emerald-200 dark:bg-emerald-500/10 dark:text-emerald-200 dark:ring-emerald-500/30'
    : value === 'disabled' || value === 'unhealthy' || value === 'failed'
      ? 'bg-rose-50 text-rose-700 ring-rose-200 dark:bg-rose-500/10 dark:text-rose-200 dark:ring-rose-500/30'
      : 'bg-amber-50 text-amber-700 ring-amber-200 dark:bg-amber-500/10 dark:text-amber-200 dark:ring-amber-500/30';
  return (
    <span className={`inline-flex rounded-full px-2.5 py-1 text-xs font-semibold ring-1 ${tone}`}>
      {labelKey ? t(labelKey) : value}
    </span>
  );
}

function modelMappingInputFromForm(formData: FormData, t: TranslationFunction): ModelMappingCreateInput {
  const errors = createEmptyModelMappingFormErrors();
  const sourceVendorCode = readRequiredFormString(formData, 'sourceVendorCode', t('admin.model.mapping.errors.sourceVendorRequired'), errors);
  const targetVendorCode = readRequiredFormString(formData, 'targetVendorCode', t('admin.model.mapping.errors.targetVendorRequired'), errors);
  const bindings = readMappingBindingsFromForm(formData, errors, t);
  const rows = readMappingRowsFromForm(formData, errors, t);
  validateUniqueModelMappingRows(rows, errors, t);
  throwModelMappingValidationErrorIfNeeded(errors, t);
  return {
    sourceVendorCode,
    targetVendorCode,
    mappingMode: 'alias',
    matchType: 'exact',
    enabled: true,
    bindings: bindings.map((binding): ModelMappingBindingInput => ({
      id: persistedChildId(binding.persistedId),
      bindingType: binding.bindingType,
      bindingId: binding.bindingId,
      bindingCode: binding.bindingType === 'global' ? null : binding.bindingCode,
      bindingName: binding.bindingName || null,
      enabled: binding.enabled,
    })),
    mappingItems: rows.map((row): ModelMappingRuleItemInput => ({
      id: persistedChildId(row.persistedId),
      sourceModel: row.sourceModel,
      targetModel: row.targetModel,
      enabled: row.enabled,
    })),
  };
}

function readMappingBindingsFromForm(formData: FormData, errors: ModelMappingFormErrors, t: TranslationFunction): ModelMappingBindingDraft[] {
  const value = readFormString(formData, 'bindingsJson');
  if (!value) {
    addModelMappingFieldError(errors, 'mappingBindings', t('admin.model.mapping.errors.bindingRequired'));
    return [];
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(value);
  } catch {
    addModelMappingFieldError(errors, 'mappingBindings', t('admin.model.mapping.errors.bindingsInvalid'));
    return [];
  }
  if (!Array.isArray(parsed)) {
    addModelMappingFieldError(errors, 'mappingBindings', t('admin.model.mapping.errors.bindingsInvalid'));
    return [];
  }
  if (parsed.length === 0) {
    addModelMappingFieldError(errors, 'mappingBindings', t('admin.model.mapping.errors.bindingRequired'));
  }
  if (parsed.length > MODEL_MAPPING_MAX_ROWS) {
    addModelMappingFieldError(errors, 'mappingBindings', t('admin.model.mapping.errors.bindingsTooMany', { count: MODEL_MAPPING_MAX_ROWS }));
  }
  const bindings = parsed.map((item, index): ModelMappingBindingDraft => {
    const fallbackId = `binding_${index}`;
    if (!item || typeof item !== 'object') {
      addModelMappingFieldError(errors, 'mappingBindings', t('admin.model.mapping.errors.bindingsInvalid'));
      return createMappingBindingDraft(null, 'global', fallbackId);
    }
    const record = item as Record<string, unknown>;
    const rowId = typeof record.id === 'string' && record.id ? record.id : fallbackId;
    const bindingType = readMappingBindingTypeValue(record.bindingType);
    const bindingCode = typeof record.bindingCode === 'string' ? record.bindingCode.trim() : '';
    const bindingId = typeof record.bindingId === 'string' && record.bindingId.trim() ? record.bindingId.trim() : null;
    const bindingName = typeof record.bindingName === 'string' ? record.bindingName.trim() : '';
    const persistedId = typeof record.persistedId === 'string' && record.persistedId.trim() ? record.persistedId.trim() : persistedChildId(rowId);
    if (bindingType !== 'global' && !bindingCode && !bindingId) {
      addModelMappingFieldError(errors, 'mappingBindings', t('admin.model.mapping.errors.bindingRequired'));
    }
    return {
      id: rowId,
      persistedId,
      bindingType,
      bindingId,
      bindingCode,
      bindingName,
      enabled: typeof record.enabled === 'boolean' ? record.enabled : true,
    };
  });
  validateUniqueModelMappingBindings(bindings, errors, t);
  return bindings;
}

function readMappingRowsFromForm(formData: FormData, errors: ModelMappingFormErrors, t: TranslationFunction): ModelMappingRowDraft[] {
  const value = readFormString(formData, 'rowsJson');
  if (!value) {
    addModelMappingFieldError(errors, 'mappingRows', t('admin.model.mapping.errors.rowsRequired'));
    return [];
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(value);
  } catch {
    addModelMappingFieldError(errors, 'mappingRows', t('admin.model.mapping.errors.rowsInvalid'));
    return [];
  }
  if (!Array.isArray(parsed)) {
    addModelMappingFieldError(errors, 'mappingRows', t('admin.model.mapping.errors.rowsInvalid'));
    return [];
  }
  if (parsed.length > MODEL_MAPPING_MAX_ROWS) {
    addModelMappingFieldError(errors, 'mappingRows', t('admin.model.mapping.errors.rowsTooMany', { count: MODEL_MAPPING_MAX_ROWS }));
  }
  const rows = parsed
    .map((item, index): ModelMappingRowDraft => {
      const fallbackId = `row_${index}`;
      if (!item || typeof item !== 'object') {
        addModelMappingRowError(errors, fallbackId, 'sourceModel', t('admin.model.mapping.errors.rowInvalid', { index: index + 1 }));
        addModelMappingRowError(errors, fallbackId, 'targetModel', t('admin.model.mapping.errors.rowInvalid', { index: index + 1 }));
        return { id: fallbackId, persistedId: null, sourceModel: '', targetModel: '', enabled: true };
      }
      const record = item as Record<string, unknown>;
      const rowId = typeof record.id === 'string' && record.id ? record.id : fallbackId;
      const persistedId = typeof record.persistedId === 'string' && record.persistedId.trim() ? record.persistedId.trim() : persistedChildId(rowId);
      const sourceModel = typeof record.sourceModel === 'string' ? record.sourceModel.trim() : '';
      const targetModel = typeof record.targetModel === 'string' ? record.targetModel.trim() : '';
      if (!sourceModel) {
        addModelMappingRowError(errors, rowId, 'sourceModel', t('admin.model.mapping.errors.sourceModelRequired'));
      }
      if (!targetModel) {
        addModelMappingRowError(errors, rowId, 'targetModel', t('admin.model.mapping.errors.targetModelRequired'));
      }
      validateModelMappingModelValue(sourceModel, t('admin.model.mapping.form.sourceModel'), errors, rowId, 'sourceModel', t);
      validateModelMappingModelValue(targetModel, t('admin.model.mapping.form.targetModel'), errors, rowId, 'targetModel', t);
      return {
        id: rowId,
        persistedId,
        sourceModel,
        targetModel,
        enabled: typeof record.enabled === 'boolean' ? record.enabled : true,
      };
    });
  if (rows.length === 0) {
    addModelMappingFieldError(errors, 'mappingRows', t('admin.model.mapping.errors.oneRowRequired'));
  }
  return rows;
}

function validateModelMappingModelValue(
  value: string,
  label: string,
  errors: ModelMappingFormErrors,
  rowId: string,
  field: ModelMappingRowFieldKey,
  t: TranslationFunction,
): void {
  if (value.length > MODEL_MAPPING_MODEL_VALUE_MAX_LENGTH) {
    addModelMappingRowError(errors, rowId, field, t('admin.model.mapping.errors.modelTooLong', { label, count: MODEL_MAPPING_MODEL_VALUE_MAX_LENGTH }));
  }
}

function validateUniqueModelMappingRows(rows: readonly ModelMappingRowDraft[], errors: ModelMappingFormErrors, t: TranslationFunction): void {
  const seen = new Set<string>();
  for (const row of rows) {
    if (!row.sourceModel) {
      continue;
    }
    const sourceModel = row.sourceModel.toLowerCase();
    if (seen.has(sourceModel)) {
      addModelMappingRowError(errors, row.id, 'sourceModel', t('admin.model.mapping.errors.duplicateSourceModel'));
      continue;
    }
    seen.add(sourceModel);
  }
}

function validateUniqueModelMappingBindings(bindings: readonly ModelMappingBindingDraft[], errors: ModelMappingFormErrors, t: TranslationFunction): void {
  const seen = new Set<string>();
  for (const binding of bindings) {
    const identity = `${binding.bindingType}:${binding.bindingType === 'global' ? 'global' : (binding.bindingId || binding.bindingCode).toLowerCase()}`;
    if (seen.has(identity)) {
      addModelMappingFieldError(errors, 'mappingBindings', t('admin.model.mapping.errors.duplicateBinding'));
      continue;
    }
    seen.add(identity);
  }
}

class ModelMappingFormValidationError extends Error {
  readonly errors: ModelMappingFormErrors;

  constructor(errors: ModelMappingFormErrors) {
    super(errors.message);
    this.name = 'ModelMappingFormValidationError';
    this.errors = errors;
  }
}

function createEmptyModelMappingFormErrors(): ModelMappingFormErrors {
  return {
    message: '',
    fieldErrors: {},
    rowErrors: {},
    firstErrorKey: null,
  };
}

function addModelMappingFieldError(errors: ModelMappingFormErrors, field: ModelMappingFieldErrorKey, message: string): void {
  if (!errors.fieldErrors[field]) {
    errors.fieldErrors[field] = message;
  }
  if (!errors.firstErrorKey) {
    errors.firstErrorKey = field;
  }
}

function addModelMappingRowError(
  errors: ModelMappingFormErrors,
  rowId: string,
  field: ModelMappingRowFieldKey,
  message: string,
): void {
  errors.rowErrors[rowId] = {
    ...errors.rowErrors[rowId],
    [field]: errors.rowErrors[rowId]?.[field] ?? message,
  };
  if (!errors.firstErrorKey) {
    errors.firstErrorKey = `${rowId}.${field}`;
  }
}

function modelMappingFormErrorsFromError(error: unknown, t: TranslationFunction): ModelMappingFormErrors {
  if (error instanceof ModelMappingFormValidationError) {
    return error.errors;
  }
  return {
    message: error instanceof Error ? error.message : t('admin.model.mapping.errors.saveMapping'),
    fieldErrors: {},
    rowErrors: {},
    firstErrorKey: null,
  };
}

function throwModelMappingValidationErrorIfNeeded(errors: ModelMappingFormErrors, t: TranslationFunction): void {
  if (Object.keys(errors.fieldErrors).length === 0 && Object.keys(errors.rowErrors).length === 0) {
    return;
  }
  throw new ModelMappingFormValidationError({
    ...errors,
    message: t('admin.model.mapping.errors.fixFields'),
  });
}

function clearModelMappingFormFieldError(
  errors: ModelMappingFormErrors | null,
  field: ModelMappingFieldErrorKey,
  t: TranslationFunction,
): ModelMappingFormErrors | null {
  if (!errors?.fieldErrors[field]) {
    return errors;
  }
  const fieldErrors = { ...errors.fieldErrors };
  delete fieldErrors[field];
  return normalizeModelMappingFormErrors({ ...errors, fieldErrors }, t);
}

function clearModelMappingFormRowError(
  errors: ModelMappingFormErrors | null,
  rowId: string,
  field: ModelMappingRowFieldKey,
  t: TranslationFunction,
): ModelMappingFormErrors | null {
  if (!errors?.rowErrors[rowId]?.[field]) {
    return errors;
  }
  const rowFieldErrors = { ...errors.rowErrors[rowId] };
  delete rowFieldErrors[field];
  const rowErrors = { ...errors.rowErrors };
  if (Object.keys(rowFieldErrors).length > 0) {
    rowErrors[rowId] = rowFieldErrors;
  } else {
    delete rowErrors[rowId];
  }
  return normalizeModelMappingFormErrors({ ...errors, rowErrors }, t);
}

function normalizeModelMappingFormErrors(errors: ModelMappingFormErrors, t: TranslationFunction): ModelMappingFormErrors | null {
  if (Object.keys(errors.fieldErrors).length === 0 && Object.keys(errors.rowErrors).length === 0) {
    return null;
  }
  return {
    ...errors,
    message: errors.message || t('admin.model.mapping.errors.fixFields'),
    firstErrorKey: null,
  };
}

function createMappingRowDraft(item: ModelMappingRule['mappingItems'][number] | null): ModelMappingRowDraft {
  return {
    id: item?.id ?? createMappingRowId(),
    persistedId: item?.id ?? null,
    sourceModel: item?.sourceModel ?? '',
    targetModel: item?.targetModel ?? '',
    enabled: item?.enabled ?? true,
  };
}

function createMappingRowDrafts(mapping: ModelMappingRule | null): ModelMappingRowDraft[] {
  if (!mapping || mapping.mappingItems.length === 0) {
    return [createMappingRowDraft(null)];
  }
  return mapping.mappingItems.map((item) => createMappingRowDraft(item));
}

function createMappingRowId(): string {
  return `row_${nextModelMappingDraftId()}`;
}

function createMappingBindingDraft(
  binding: ModelMappingRule['bindings'][number] | null,
  fallbackType: ModelMappingRule['bindingType'],
  fallbackId?: string,
): ModelMappingBindingDraft {
  return {
    id: binding?.id ?? fallbackId ?? createMappingBindingId(),
    persistedId: binding?.id ?? null,
    bindingType: binding?.bindingType ?? fallbackType,
    bindingId: binding?.bindingId ?? null,
    bindingCode: binding?.bindingCode ?? '',
    bindingName: binding?.bindingName ?? '',
    enabled: binding?.enabled ?? true,
  };
}

function createMappingBindingDrafts(mapping: ModelMappingRule | null, fallbackType: ModelMappingRule['bindingType']): ModelMappingBindingDraft[] {
  if (!mapping || mapping.bindings.length === 0) {
    return [createMappingBindingDraft(null, fallbackType)];
  }
  return mapping.bindings.map((binding) => createMappingBindingDraft(binding, mapping.bindingType));
}

function createMappingBindingId(): string {
  return `binding_${nextModelMappingDraftId()}`;
}

function nextModelMappingDraftId(): string {
  nextModelMappingDraftSequence += 1;
  return nextModelMappingDraftSequence.toString(36);
}

function normalizeBindingDraftForType(binding: ModelMappingBindingDraft, sourceCode: string): ModelMappingBindingDraft {
  if (binding.bindingType === 'global') {
    return { ...binding, bindingId: null, bindingCode: '', bindingName: '' };
  }
  if (binding.bindingType === 'vendor' && !binding.bindingCode.trim() && sourceCode.trim()) {
    return { ...binding, bindingCode: sourceCode.trim(), bindingName: sourceCode.trim() };
  }
  return binding;
}

function persistedChildId(value: string | null | undefined): string | null {
  if (!value || /^row_/u.test(value) || /^binding_/u.test(value)) {
    return null;
  }
  return value;
}

function writeHiddenFormValue(form: HTMLFormElement, name: string, value: string): void {
  const input = form.querySelector<HTMLInputElement>(`input[name="${name}"]`);
  if (input) {
    input.value = value;
  }
}

function readMappingPrimaryBindingType(formData: FormData): ModelMappingRule['bindingType'] {
  const value = readFormString(formData, 'bindingType');
  return readMappingBindingTypeValue(value);
}

function readMappingBindingTypeValue(value: unknown): ModelMappingRule['bindingType'] {
  if (
    value === 'global'
    || value === 'vendor'
    || value === 'channel_group'
    || value === 'channel'
    || value === 'provider_account'
    || value === 'site'
    || value === 'site_service'
  ) {
    return value;
  }
  return 'global';
}

function mappingBindingIdentity(mapping: ModelMappingRule, t: TranslationFunction): string {
  const binding = mapping.bindings[0];
  if (!binding || binding.bindingType === 'global') {
    return t('admin.model.mapping.allRequests');
  }
  return binding.bindingName || binding.bindingCode || binding.bindingId || t('admin.model.mapping.noData');
}

function readFormString(formData: FormData, name: string): string {
  const value = formData.get(name);
  return typeof value === 'string' ? value.trim() : '';
}

function readOptionalFormString(formData: FormData, name: string): string | null {
  const value = readFormString(formData, name);
  return value || null;
}

function readRequiredFormString(
  formData: FormData,
  name: ModelMappingFieldErrorKey,
  message: string,
  errors: ModelMappingFormErrors,
): string {
  const value = readFormString(formData, name);
  if (!value) {
    addModelMappingFieldError(errors, name, message);
  }
  return value;
}
