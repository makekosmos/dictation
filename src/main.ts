import { createApp, h, ref } from "vue";
import App from "./App.vue";
import DictationPillView from "./DictationPillView.vue";
import "./styles.css";

const hash = ref(window.location.hash);
window.addEventListener("hashchange", () => {
  hash.value = window.location.hash;
});

createApp({
  render: () => h(hash.value.startsWith("#dictation-pill") ? DictationPillView : App),
}).mount("#app");
