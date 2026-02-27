import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { Waveform } from './Waveform';

describe('Overlay Waveform', () => {
  it('renders 12 bar elements', () => {
    const { container } = render(<Waveform active={true} level={0.5} />);
    const bars = container.querySelectorAll('span');
    expect(bars.length).toBe(12);
  });

  it('is hidden from screen readers via aria-hidden', () => {
    const { container } = render(<Waveform active={false} level={0} />);
    const wrapper = container.firstElementChild;
    expect(wrapper?.getAttribute('aria-hidden')).toBe('true');
  });

  it('uses green color when active', () => {
    const { container } = render(<Waveform active={true} level={0.5} />);
    const bar = container.querySelector('span');
    expect(bar?.style.backgroundColor).toContain('164, 244, 194');
  });

  it('uses gray color when inactive', () => {
    const { container } = render(<Waveform active={false} level={0.5} />);
    const bar = container.querySelector('span');
    expect(bar?.style.backgroundColor).toContain('140, 140, 140');
  });

  it('bars are taller at higher levels when active', () => {
    const { container: lowContainer } = render(<Waveform active={true} level={0.1} />);
    const { container: highContainer } = render(<Waveform active={true} level={1.0} />);

    const lowBars = lowContainer.querySelectorAll('span');
    const highBars = highContainer.querySelectorAll('span');

    // Compare the center bar (index 5, factor=1.0) heights
    const lowHeight = parseInt(lowBars[5].style.height);
    const highHeight = parseInt(highBars[5].style.height);
    expect(highHeight).toBeGreaterThan(lowHeight);
  });

  it('clamps level to 0-1 range', () => {
    const { container: overContainer } = render(<Waveform active={true} level={5.0} />);
    const { container: normalContainer } = render(<Waveform active={true} level={1.0} />);

    // Bars should be the same height for level=5.0 and level=1.0
    const overBars = overContainer.querySelectorAll('span');
    const normalBars = normalContainer.querySelectorAll('span');
    expect(overBars[5].style.height).toBe(normalBars[5].style.height);
  });

  it('clamps negative level to zero', () => {
    const { container: negContainer } = render(<Waveform active={true} level={-1} />);
    const { container: zeroContainer } = render(<Waveform active={true} level={0} />);

    const negBars = negContainer.querySelectorAll('span');
    const zeroBars = zeroContainer.querySelectorAll('span');
    expect(negBars[5].style.height).toBe(zeroBars[5].style.height);
  });

  it('handles NaN and Infinity gracefully', () => {
    const { container: nanContainer } = render(<Waveform active={true} level={NaN} />);
    const { container: infContainer } = render(<Waveform active={true} level={Infinity} />);

    // Both should render without errors
    expect(nanContainer.querySelectorAll('span').length).toBe(12);
    expect(infContainer.querySelectorAll('span').length).toBe(12);
  });

  it('uses CSS transitions for smooth animation', () => {
    const { container } = render(<Waveform active={true} level={0.5} />);
    const bar = container.querySelector('span');
    expect(bar?.style.transition).toContain('height');
  });

  it('bars have minimum height of 2px', () => {
    const { container } = render(<Waveform active={false} level={0} />);
    const bars = container.querySelectorAll('span');
    for (const bar of bars) {
      const height = parseInt(bar.style.height);
      expect(height).toBeGreaterThanOrEqual(2);
    }
  });
});
