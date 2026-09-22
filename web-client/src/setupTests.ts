import '@testing-library/jest-dom'
import { cleanup } from '@testing-library/react'
import { afterEach } from 'vitest'

// Automatically unmount React trees after each test
afterEach(() => {
  cleanup()
})
