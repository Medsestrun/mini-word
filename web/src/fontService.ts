export interface FontMetrics {
  lineHeight: number;
  charWidths: Float32Array;
  defaultWidth: number;
}

export class FontService {
  private canvas: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D;
  private fontCache: Map<string, number> = new Map();
  private metricsCache: Map<number, FontMetrics> = new Map();
  private fontDetails: Map<number, { family: string, size: number }> = new Map();
  private fontStrings: Map<number, string> = new Map();
  private nextId = 0; // Start at 0

  constructor() {
    this.canvas = document.createElement('canvas');
    const ctx = this.canvas.getContext('2d');
    if (!ctx) {
      throw new Error('Could not get 2D context');
    }
    this.ctx = ctx;
  }

  getOrRegisterFont(fontFamily: string, fontSize: number): { id: number, metrics: FontMetrics, isNew: boolean } {
    const key = `${fontFamily}:${fontSize}`;
    if (this.fontCache.has(key)) {
      const id = this.fontCache.get(key)!;
      return { id, metrics: this.metricsCache.get(id)!, isNew: false };
    }

    const id = this.nextId++;
    const metrics = this.measureFont(fontFamily, fontSize);
    
    this.fontCache.set(key, id);
    this.metricsCache.set(id, metrics);
    this.fontStrings.set(id, `${fontSize}px "${fontFamily}"`);
    this.fontDetails.set(id, { family: fontFamily, size: fontSize });

    return { id, metrics, isNew: true };
  }

  getFontString(id: number): string | undefined {
    return this.fontStrings.get(id);
  }

  getFontDetails(id: number): { family: string, size: number } | undefined {
    return this.fontDetails.get(id);
  }

  measureFont(fontFamily: string, fontSize: number): FontMetrics {
    this.ctx.font = `${fontSize}px "${fontFamily}"`;
    
    // Measure common ASCII characters (32-126)
    // We allocation 128 to cover 0-127
    const charWidths = new Float32Array(128);
    
    // Default width (usually 'M' or 'W' or average, but for monospace 'M' is safe)
    const defaultMetrics = this.ctx.measureText('M');
    const defaultWidth = defaultMetrics.width;

    for (let i = 0; i < 128; i++) {
      if (i < 32) {
        // Control characters
        charWidths[i] = 0;
      } else {
        const char = String.fromCharCode(i);
        const metrics = this.ctx.measureText(char);
        charWidths[i] = metrics.width;
      }
    }

    // Heuristic for line height: 1.2 * fontSize is standard for browsers
    // Ideally we would use TextMetrics.fontBoundingBoxAscent + Descent but support varies
    const lineHeight = fontSize * 1.2;

    return {
      lineHeight,
      charWidths,
      defaultWidth
    };
  }
}

export const fontService = new FontService();
