import React, { useState, useEffect, useCallback, useRef, useMemo } from 'react';
import { Check, ChevronsUpDown, Loader2, AlertCircle } from 'lucide-react';
import { cn } from '@/lib/utils';
import { Button } from '@/components/ui/button';
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from '@/components/ui/command';
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover';
import { useDebounce } from '@/hooks/useDebounce';

interface ModelInfo {
  id: string;
  object?: string;
  created?: number;
  owned_by?: string;
}

interface ModelComboboxProps {
  value: string;
  onChange: (value: string) => void;
  onSelect?: (value: string) => void;
  onBlur?: () => void;
  apiUrl: string;
  apiKey: string;
  disabled?: boolean;
  placeholder?: string;
  className?: string;
}

const DEBOUNCE_DELAY_MS = 500;
const FETCH_TIMEOUT_MS = 5000;
const MAX_DISPLAYED_MODELS = 50;

function normalizeModel(model: ModelInfo | string): ModelInfo {
  return typeof model === 'string' ? { id: model } : model;
}

function extractBaseUrl(url: string): string {
  if (!url) return '';
  
  try {
    const urlObj = new URL(url);
    const pathParts = urlObj.pathname.split('/').filter(Boolean);
    
    const chatCompletionsIndex = pathParts.findIndex((p, i) => 
      p === 'chat' && pathParts[i + 1] === 'completions'
    );
    if (chatCompletionsIndex !== -1) {
      urlObj.pathname = '/' + pathParts.slice(0, chatCompletionsIndex).join('/');
      return urlObj.origin + urlObj.pathname;
    }
    
    if (pathParts[pathParts.length - 1] === 'completions') {
      urlObj.pathname = '/' + pathParts.slice(0, -1).join('/');
      return urlObj.origin + urlObj.pathname;
    }
    
    return urlObj.origin + urlObj.pathname;
  } catch (error) {
    console.debug('extractBaseUrl: Invalid URL format', { url, error });
    return url.replace(/\/chat\/completions\/?$/, '').replace(/\/completions\/?$/, '');
  }
}

function generateModelEndpoints(apiUrl: string): string[] {
  if (!apiUrl) return [];
  
  const baseUrl = extractBaseUrl(apiUrl);
  const endpoints: string[] = [];
  
  try {
    const urlObj = new URL(baseUrl);
    const origin = urlObj.origin;
    const pathParts = urlObj.pathname.split('/').filter(Boolean);
    
    endpoints.push(`${baseUrl}/models`);
    
    const versionIndex = pathParts.findIndex(p => /^v\d+$/.test(p));
    if (versionIndex !== -1) {
      const beforeVersion = pathParts.slice(0, versionIndex).join('/');
      const version = pathParts[versionIndex];
      if (beforeVersion) {
        endpoints.push(`${origin}/${beforeVersion}/${version}/models`);
        endpoints.push(`${origin}/${beforeVersion}/models`);
      }
      endpoints.push(`${origin}/${version}/models`);
    }
    
    endpoints.push(`${origin}/v1/models`);
    endpoints.push(`${origin}/api/v1/models`);
    endpoints.push(`${origin}/openai/v1/models`);
    endpoints.push(`${origin}/models`);
    endpoints.push(`${origin}/api/tags`);
    
  } catch (error) {
    endpoints.push(`${baseUrl}/models`);
  }
  
  return [...new Set(endpoints.filter(Boolean))];
}

async function fetchModels(apiUrl: string, apiKey: string, signal: AbortSignal): Promise<ModelInfo[]> {
  if (!apiUrl) return [];
  
  const headers: HeadersInit = {
    'Content-Type': 'application/json',
  };
  
  if (apiKey) {
    headers['Authorization'] = `Bearer ${apiKey}`;
  }
  
  const endpoints = generateModelEndpoints(apiUrl);
  
  for (const endpoint of endpoints) {
    try {
      const response = await fetch(endpoint, {
        method: 'GET',
        headers,
        signal: AbortSignal.any([signal, AbortSignal.timeout(FETCH_TIMEOUT_MS)]),
      });
      
      if (!response.ok) continue;
      
      const data = await response.json();
      
      if (data.data && Array.isArray(data.data)) {
        return data.data.map(normalizeModel);
      }
      
      if (data.models && Array.isArray(data.models)) {
        return data.models.map((m: ModelInfo | { name: string }) => 
          'name' in m && !('id' in m) ? { id: m.name, ...m } : normalizeModel(m as ModelInfo)
        );
      }
      
      if (Array.isArray(data)) {
        return data.map(normalizeModel);
      }
      
      if (data.object === 'list' && data.data) {
        return data.data.map(normalizeModel);
      }
    } catch (error) {
      if (signal.aborted) throw error;
      console.debug('fetchModels: Endpoint failed', { endpoint, error });
      continue;
    }
  }
  
  return [];
}

export function ModelCombobox({
  value,
  onChange,
  onSelect,
  onBlur,
  apiUrl,
  apiKey,
  disabled = false,
  placeholder = '选择或输入模型名称',
  className,
}: ModelComboboxProps) {
  const [open, setOpen] = useState(false);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [inputValue, setInputValue] = useState(value);
  const fetchIdRef = useRef(0);
  const abortControllerRef = useRef<AbortController | null>(null);
  
  const debouncedApiUrl = useDebounce(apiUrl, DEBOUNCE_DELAY_MS);
  const debouncedApiKey = useDebounce(apiKey, DEBOUNCE_DELAY_MS);
  
  useEffect(() => {
    setInputValue(value);
  }, [value]);
  
  const loadModels = useCallback(async () => {
    abortControllerRef.current?.abort();
    abortControllerRef.current = null;

    if (disabled || !debouncedApiUrl) {
      fetchIdRef.current += 1;
      setModels([]);
      setIsLoading(false);
      setError(null);
      return;
    }
    
    const currentFetchId = ++fetchIdRef.current;
    const abortController = new AbortController();
    abortControllerRef.current = abortController;
    setIsLoading(true);
    setError(null);
    
    try {
      const fetchedModels = await fetchModels(
        debouncedApiUrl,
        debouncedApiKey,
        abortController.signal,
      );
      
      if (currentFetchId !== fetchIdRef.current) return;
      
      setModels(fetchedModels);
      if (fetchedModels.length === 0) {
        setError('无法获取模型列表');
      }
    } catch (error) {
      console.debug('loadModels: Failed to fetch models', error);
      if (currentFetchId !== fetchIdRef.current) return;
      setModels([]);
      setError('获取失败');
    } finally {
      if (abortControllerRef.current === abortController) {
        abortControllerRef.current = null;
      }
      if (currentFetchId === fetchIdRef.current) {
        setIsLoading(false);
      }
    }
  }, [debouncedApiUrl, debouncedApiKey, disabled]);
  
  useEffect(() => {
    loadModels();
    return () => abortControllerRef.current?.abort();
  }, [loadModels]);
  
  const handleSelect = useCallback((selectedValue: string) => {
    onChange(selectedValue);
    setInputValue(selectedValue);
    setOpen(false);
    if (onSelect) {
      onSelect(selectedValue);
    }
  }, [onChange, onSelect]);
  
  const handleInputChange = useCallback((newValue: string) => {
    setInputValue(newValue);
    onChange(newValue);
  }, [onChange]);
  
  const handleOpenChange = useCallback((newOpen: boolean) => {
    setOpen(newOpen);
    if (!newOpen && onBlur) {
      onBlur();
    }
  }, [onBlur]);
  
  const filteredModels = useMemo(() => 
    models.filter(model => 
      model.id.toLowerCase().includes(inputValue.toLowerCase())
    ),
    [models, inputValue]
  );
  
  const showCreateOption = useMemo(() => 
    inputValue && !models.some(model => 
      model.id.toLowerCase() === inputValue.toLowerCase()
    ),
    [inputValue, models]
  );

  return (
    <Popover open={open} onOpenChange={handleOpenChange}>
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          role="combobox"
          aria-expanded={open}
          disabled={disabled}
          className={cn(
            'w-full justify-between bg-white/50 font-normal',
            !value && 'text-muted-foreground',
            className
          )}
        >
          <span className="truncate">
            {value || placeholder}
          </span>
          <div className="flex items-center gap-1 ml-2 shrink-0">
            {isLoading && <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />}
            {error && !isLoading && models.length === 0 && (
              <AlertCircle className="h-4 w-4 text-amber-500">
                <title>{error}</title>
              </AlertCircle>
            )}
            <ChevronsUpDown className="h-4 w-4 opacity-50" />
          </div>
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-[--radix-popover-trigger-width] p-0" align="start">
        <Command shouldFilter={false}>
          <CommandInput
            placeholder="搜索或输入模型名称..."
            value={inputValue}
            onValueChange={handleInputChange}
          />
          <CommandList>
            {isLoading ? (
              <div className="flex items-center justify-center py-6 text-sm text-muted-foreground">
                <Loader2 className="h-4 w-4 animate-spin mr-2" />
                正在获取模型列表...
              </div>
            ) : (
              <>
                {filteredModels.length === 0 && !showCreateOption && (
                  <CommandEmpty>
                    {error || '未找到匹配的模型'}
                  </CommandEmpty>
                )}
                
                {showCreateOption && (
                  <CommandGroup heading="手动输入">
                    <CommandItem
                      value={inputValue}
                      onSelect={() => handleSelect(inputValue)}
                    >
                      <Check
                        className={cn(
                          'mr-2 h-4 w-4',
                          value === inputValue ? 'opacity-100' : 'opacity-0'
                        )}
                      />
                      使用 "{inputValue}"
                    </CommandItem>
                  </CommandGroup>
                )}
                
                {filteredModels.length > 0 && (
                  <CommandGroup heading={`可用模型 (${filteredModels.length})`}>
                    {filteredModels.slice(0, MAX_DISPLAYED_MODELS).map((model) => (
                      <CommandItem
                        key={model.id}
                        value={model.id}
                        onSelect={() => handleSelect(model.id)}
                      >
                        <Check
                          className={cn(
                            'mr-2 h-4 w-4',
                            value === model.id ? 'opacity-100' : 'opacity-0'
                          )}
                        />
                        <span className="truncate">{model.id}</span>
                        {model.owned_by && (
                          <span className="ml-auto text-xs text-muted-foreground">
                            {model.owned_by}
                          </span>
                        )}
                      </CommandItem>
                    ))}
                    {filteredModels.length > MAX_DISPLAYED_MODELS && (
                      <div className="py-2 px-2 text-xs text-muted-foreground text-center">
                        还有 {filteredModels.length - MAX_DISPLAYED_MODELS} 个模型，请输入关键词筛选
                      </div>
                    )}
                  </CommandGroup>
                )}
              </>
            )}
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}
