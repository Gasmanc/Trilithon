import { describe, it, expect } from 'vitest';
import { App } from './App';
import { render, screen } from '@testing-library/react';

describe('App', () => {
  it('renders the Trilithon heading', () => {
    render(<App />);
    expect(screen.getByText('Trilithon')).toBeInTheDocument();
  });
});
