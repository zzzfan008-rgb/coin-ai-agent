import { describe, it, expect, vi } from 'vitest'
import { streamChatCompletion } from '../api/client'

describe('streamChatCompletion', () => {
  const createMockResponse = (chunks: string[]) => {
    const encoder = new TextEncoder()
    let chunkIndex = 0
    
    const readable = new ReadableStream({
      pull(ctrl) {
        if (chunkIndex < chunks.length) {
          ctrl.enqueue(encoder.encode(chunks[chunkIndex]))
          chunkIndex++
        } else {
          ctrl.close()
        }
      },
    })

    return new Response(readable, {
      status: 200,
      headers: { 'Content-Type': 'text/event-stream' },
    })
  }

  it('parses valid SSE frames', async () => {
    const sse = 'data: {"choices":[{"delta":{"content":"Hello"}}]}\n\ndata: {"choices":[{"delta":{"content":" World"}}]}\n\ndata: [DONE]\n\n'
    const mockRes = createMockResponse([sse])
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(mockRes)

    const deltas: string[] = []
    const result = await streamChatCompletion(
      { messages: [{ role: 'user', content: 'Hi' }] },
      (d) => deltas.push(d),
    )

    expect(deltas).toEqual(['Hello', ' World'])
    expect(result).toBe('Hello World')
  })

  it('handles incomplete frames', async () => {
    const part1 = 'data: {"choices":[{"delta":{"content":"He'
    const part2 = 'llo"}}]}\n\ndata: [DONE]\n\n'
    const mockRes = createMockResponse([part1, part2])
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(mockRes)

    const deltas: string[] = []
    const result = await streamChatCompletion(
      { messages: [{ role: 'user', content: 'Hi' }] },
      (d) => deltas.push(d),
    )

    expect(result).toBe('Hello')
  })

  it('ignores non-JSON comment lines', async () => {
    const sse = ': heartbeat\n\ndata: {"choices":[{"delta":{"content":"OK"}}]}\n\ndata: [DONE]\n\n'
    const mockRes = createMockResponse([sse])
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(mockRes)

    const deltas: string[] = []
    const result = await streamChatCompletion(
      { messages: [{ role: 'user', content: 'Hi' }] },
      (d) => deltas.push(d),
    )

    expect(result).toBe('OK')
  })

  it('throws on non-ok response', async () => {
    const mockRes = new Response('{"error":{"message":"Unauthorized"}}', { status: 401 })
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(mockRes)

    await expect(
      streamChatCompletion({ messages: [{ role: 'user', content: 'Hi' }] }, () => {}),
    ).rejects.toThrow()
  })
})
