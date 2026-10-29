<script lang="ts">
interface Props {
  name: string;
  busy: boolean;
  cancel: () => void;
  discard: () => void;
  save: () => void;
}

let { name, busy, cancel, discard, save }: Props = $props();
let dialog = $state<HTMLDialogElement>();

$effect(() => {
  dialog?.showModal();
});
</script>

<dialog
  bind:this={dialog}
  oncancel={(event) => {
    event.preventDefault();
    cancel();
  }}
  aria-labelledby="close-title"
  aria-describedby="close-description"
>
  <h1 id="close-title">Save changes to {name}?</h1>
  <p id="close-description">Your changes will be lost if you don’t save them.</p>
  <div class="dialog-actions">
    <button type="button" disabled={busy} onclick={cancel}>Cancel</button>
    <button type="button" disabled={busy} onclick={discard}>Don’t Save</button>
    <button type="button" class="primary" disabled={busy} onclick={save}>Save</button>
  </div>
</dialog>
