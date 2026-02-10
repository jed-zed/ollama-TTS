import { useRef, useState, useCallback, useEffect } from 'react';
import { cn } from '@/lib/utils';

interface ColorPickerProps {
  value: string;
  onChange: (color: string) => void;
  className?: string;
  disabled?: boolean;
}

const ColorPicker = ({ value, onChange, className, disabled = false }: ColorPickerProps) => {
  const inputRef = useRef<HTMLInputElement>(null);
  const normalizedValue = normalizeToHex(value);
  const [localColor, setLocalColor] = useState(normalizedValue);

  useEffect(() => {
    setLocalColor(normalizeToHex(value));
  }, [value]);

  const handleClick = useCallback(() => {
    if (!disabled) {
      inputRef.current?.click();
    }
  }, [disabled]);

  const handleInput = useCallback((e: React.FormEvent<HTMLInputElement>) => {
    setLocalColor((e.target as HTMLInputElement).value);
  }, []);

  const handleChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const color = e.target.value;
    setLocalColor(color);
    onChange(color);
  }, [onChange]);

  return (
    <div className={cn('relative', className)}>
      <button
        type="button"
        onClick={handleClick}
        disabled={disabled}
        className={cn(
          'w-9 h-9 rounded-md border-2 border-border shadow-sm',
          'hover:border-primary/50 hover:shadow-md transition-all',
          'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
          'disabled:opacity-50 disabled:cursor-not-allowed',
        )}
        style={{ backgroundColor: localColor }}
        title="点击选择颜色"
        aria-label="选择颜色"
      />
      <input
        ref={inputRef}
        type="color"
        value={localColor}
        onInput={handleInput}
        onChange={handleChange}
        className="sr-only"
        tabIndex={-1}
        aria-hidden="true"
      />
    </div>
  );
};

function normalizeToHex(color: string): string {
  if (!color) return '#000000';

  if (/^#[0-9a-fA-F]{6}$/.test(color)) return color;
  if (/^#[0-9a-fA-F]{3}$/.test(color)) {
    const [, r, g, b] = color.match(/^#(.)(.)(.)$/) || [];
    return `#${r}${r}${g}${g}${b}${b}`;
  }

  const rgbaMatch = color.match(/rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)/);
  if (rgbaMatch) {
    const [, r, g, b] = rgbaMatch;
    return `#${Number(r).toString(16).padStart(2, '0')}${Number(g).toString(16).padStart(2, '0')}${Number(b).toString(16).padStart(2, '0')}`;
  }

  if (color === 'transparent') return '#000000';
  if (color.startsWith('linear-gradient')) return '#000000';

  return '#000000';
}

export default ColorPicker;
